<?php
declare(strict_types=1);

/**
 * The bot's API.
 *
 * Everything the Discord bot needs, over HTTP, behind the same shared bearer
 * token stats/ingest.php uses. The bot never holds database credentials and never
 * writes a row itself: all the rules - who is who, which fixture a clip belongs
 * to, what a stat means - stay here, in the code that already knows them.
 *
 * POST JSON, always answers JSON:
 *
 *   {"action":"fixtures","league_id":13,"week":5}
 *       -> the week's games with the thread title, both owners' Discord ids, the
 *          box-score URL to post, and any thread already made
 *
 *   {"action":"thread_created","league_id":13,"game_id":9,"thread_id":"...",
 *    "forum_id":"...","title":"..."}
 *       -> remember it, so footage in that thread needs no guessing
 *
 *   {"action":"footage","thread_id":"...","urls":["..."],"text":"...",
 *    "msg_id":"...","posted_at":"..."}
 *       -> file the clips against that thread's fixture. league_id and
 *          thread_title are accepted instead of thread_id for a thread the bot
 *          did not create, and then the text is matched the way the sweep does.
 *
 *   {"action":"stat","league_id":13,"q":"who leads in sacks"}
 *       -> a one-line answer, or a list of what it can answer
 *   {"action":"team_home","discord_user_id":"..."}
 *       -> that person's club in the current league, for the launcher's League
 *          tab (via the Reborn backend), or {"ok":true,"member":false}
 *
 * WHAT PUBLISHES AND WHAT WAITS. A clip posted in a thread THIS SITE created is
 * published immediately: the thread-to-fixture mapping was written by the site
 * when it made the thread, so there is nothing to guess and nothing for a human
 * to check - asking someone to approve a fact the site already knows is busywork.
 * Everything else waits for review, because everything else is inferred: a
 * fuzzy title match once read the handle "pat" inside "Patriots" and pulled in a
 * club that was not playing. Guesses get a human; facts do not.
 */

require_once __DIR__ . '/../_inc/config.php';
require_once __DIR__ . '/../_inc/db.php';
require_once __DIR__ . '/../_inc/nsr_data.php';
require_once __DIR__ . '/../_inc/nsr_standings.php';
require_once __DIR__ . '/../_inc/nsr_footage.php';
require_once __DIR__ . '/../_inc/nsr_footage_sweep.php';
require_once __DIR__ . '/../_inc/nsr_threads.php';
require_once __DIR__ . '/../_inc/nsr_owners.php';

header('Content-Type: application/json');

function out(array $d): void { echo json_encode($d); exit; }
function fail(string $m, int $code = 400): void {
    http_response_code($code);
    out(['ok' => false, 'error' => $m]);
}

/* ------------------------------------------------------------------- auth -- */

$tokenFile = __DIR__ . '/../_inc/ingest_token.php';
if (!is_file($tokenFile)) fail('API not configured', 503);
require_once $tokenFile;
if (!defined('INGEST_TOKEN') || INGEST_TOKEN === '' || INGEST_TOKEN === 'CHANGEME') {
    fail('API not configured', 503);
}

$hdr = '';
foreach (['HTTP_AUTHORIZATION', 'REDIRECT_HTTP_AUTHORIZATION'] as $k) {
    if (!empty($_SERVER[$k])) { $hdr = (string)$_SERVER[$k]; break; }
}
if ($hdr === '' && function_exists('apache_request_headers')) {
    foreach (apache_request_headers() as $k => $v) {
        if (strcasecmp($k, 'Authorization') === 0) { $hdr = (string)$v; break; }
    }
}
$sent = '';
if (preg_match('/Bearer\s+(.+)/i', $hdr, $m)) $sent = trim($m[1]);
if ($sent === '') $sent = (string)($_POST['token'] ?? '');
// constant time, so a wrong token cannot be probed by timing
if (!hash_equals(INGEST_TOKEN, $sent)) fail('Unauthorized', 401);

if (($_SERVER['REQUEST_METHOD'] ?? '') !== 'POST') fail('POST required', 405);

$raw = file_get_contents('php://input') ?: '';
$in  = $raw !== '' ? json_decode($raw, true) : $_POST;
if (!is_array($in)) fail('Body is not JSON');

$pdo = db();
$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);

/* Create any missing table NOW, before an action can open a transaction. DDL
   implicitly commits in MariaDB, so a CREATE TABLE reached from inside one
   silently commits everything before it and a later rollback undoes nothing. Both
   helpers no-op once the tables exist. */
nsr_threads_schema($pdo);
nsr_footage_schema($pdo);

$action   = (string)($in['action'] ?? '');
$leagueId = (int)($in['league_id'] ?? 0);

/** The absolute site URL the bot should post, so Discord can unfurl it. */
function bot_base(): string {
    $scheme = (!empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off') ? 'https' : 'http';
    return $scheme . '://' . (string)($_SERVER['HTTP_HOST'] ?? 'localhost');
}

/* -------------------------------------------------------------- fixtures -- */

if ($action === 'fixtures') {
    if ($leagueId <= 0) fail('league_id required');
    $week = (int)($in['week'] ?? 0);

    if ($week <= 0) fail('week required');

    $teams = nsr_teams($pdo, $leagueId);

    /* Owner per club, so each side can be pinged by Discord id. An owner with no
       id mapped is named in plain text instead - a missing mapping costs a
       mention, not the thread. */
    $ownerOf = [];
    $st = $pdo->prepare(
        "SELECT ot.team_id, o.handle, o.display_name, o.discord_user_id
           FROM owner_teams ot JOIN league_owners o ON o.id = ot.owner_id
          WHERE ot.league_id = :l");
    $st->execute(['l' => $leagueId]);
    foreach ($st->fetchAll(PDO::FETCH_ASSOC) as $r) {
        $ownerOf[(int)$r['team_id']] = [
            'name'    => (string)($r['display_name'] ?: $r['handle']),
            'discord' => (string)($r['discord_user_id'] ?? ''),
        ];
    }

    $existing = nsr_threads_for($pdo, $leagueId);

    $fq = $pdo->prepare(
        "SELECT id, home_team_id, away_team_id FROM games
          WHERE league_id = :l AND week_num = :w
            AND (series_game_no = 1 OR series_game_no IS NULL)
          ORDER BY id");
    $fq->execute(['l' => $leagueId, 'w' => $week]);

    $rows = [];
    foreach ($fq->fetchAll(PDO::FETCH_ASSOC) as $f) {
        $gid = (int)$f['id'];
        $h = $teams[(int)$f['home_team_id']] ?? null;
        $a = $teams[(int)$f['away_team_id']] ?? null;
        if (!$h || !$a) continue;

        $rows[] = [
            'game_id'   => $gid,
            /* The title format the league already uses: away first, as a fixture
               is spoken. Also what the matcher reads if a thread is ever renamed. */
            'title'     => sprintf('Week %d: %s vs %s', $week, $a['name'], $h['name']),
            'home'      => ['id' => (int)$h['id'], 'name' => (string)$h['name'],
                            'owner' => $ownerOf[(int)$h['id']] ?? null],
            'away'      => ['id' => (int)$a['id'], 'name' => (string)$a['name'],
                            'owner' => $ownerOf[(int)$a['id']] ?? null],
            /* The public preview, which unfurls; box.php is login-walled. */
            'box_url'   => bot_base() . '/league/og/game.php?league_id=' . $leagueId . '&game_id=' . $gid,
            'week_label' => nsr_week_label($leagueId, $week),
            'thread_id' => $existing[$gid] ?? null,
        ];
    }
    out(['ok' => true, 'week' => $week, 'week_label' => nsr_week_label($leagueId, $week),
         'fixtures' => $rows]);
}

/* -------------------------------------------------------- thread_created -- */

if ($action === 'thread_created') {
    $gid = (int)($in['game_id'] ?? 0);
    $tid = trim((string)($in['thread_id'] ?? ''));
    if ($leagueId <= 0 || $gid <= 0 || $tid === '') fail('league_id, game_id and thread_id required');

    $chk = $pdo->prepare("SELECT COUNT(*) FROM games WHERE id = ? AND league_id = ?");
    $chk->execute([$gid, $leagueId]);
    if (!(int)$chk->fetchColumn()) fail("game $gid is not in league $leagueId");

    nsr_thread_set($pdo, $leagueId, $gid, $tid,
        ($in['forum_id'] ?? null) !== null ? (string)$in['forum_id'] : null,
        ($in['title'] ?? null) !== null ? (string)$in['title'] : null);
    out(['ok' => true, 'game_id' => $gid, 'thread_id' => $tid]);
}

/* ---------------------------------------------------------------- footage -- */

if ($action === 'footage') {
    $urls = $in['urls'] ?? [];
    if (is_string($urls)) $urls = [$urls];
    $text = (string)($in['text'] ?? '');
    if (!is_array($urls) || !$urls) {
        /* Accept a bare message and pull the links out of it, so the bot does not
           have to duplicate the link-finding regex. */
        preg_match_all('~https?://\S+~i', $text, $lm);
        $urls = $lm[0] ?? [];
    }
    $urls = array_values(array_filter(array_map('strval', $urls),
        fn($u) => nsr_youtube_id($u) !== ''));
    if (!$urls) out(['ok' => true, 'queued' => 0, 'note' => 'no video links in the message']);

    $threadId = trim((string)($in['thread_id'] ?? ''));
    $gid = 0; $sg = null; $why = ''; $how = '';

    /* A thread the bot made needs no matching at all. */
    $known = $threadId !== '' ? nsr_thread_game($pdo, $threadId) : null;
    if ($known) {
        /* If the caller ALSO named a league, the two must agree. Quietly taking
           the thread's league over the caller's turned a contradiction into a
           cross-league write: "league 13" plus a league-5 thread filed the clip
           into league 5 and reported success. A disagreement is a bug in the
           caller, so it is refused rather than resolved. */
        if ($leagueId > 0 && $leagueId !== $known['league_id']) {
            fail(sprintf('thread %s belongs to league %d, not %d',
                         $threadId, $known['league_id'], $leagueId), 409);
        }
        $leagueId = $known['league_id'];
        $gid = $known['game_id'];
        $how = 'thread';
        /* The message may still say which game of the series it is. */
        if (preg_match('/\b(?:game|g)\s*([123])\b/i', $text, $gm)) $sg = (int)$gm[1];
    } else {
        if ($leagueId <= 0) fail('league_id required for a thread I did not create');
        $hint = (string)($in['thread_title'] ?? '') . ' ' . $text;
        $m = nsr_footage_match($pdo, $leagueId, $hint, (int)($in['week'] ?? 0));
        $gid = (int)($m['game_id'] ?? 0);
        $sg  = $m['series_game_no'];
        $why = (string)$m['reason'];
        $how = $gid ? 'title' : 'unmatched';
    }

    /* A thread this site created IS the fixture - it recorded the mapping itself
       when it posted the thread - so there is no inference to check and the clip
       goes live. A title match is a guess and still waits for a human. */
    $status = $how === 'thread' ? 'approved' : 'pending';

    $queued = 0;
    foreach ($urls as $u) {
        nsr_footage_add($pdo, $leagueId, $gid, $sg, $u,
            mb_substr(trim(($in['thread_title'] ?? '') . ' ' . $text), 0, 300),
            'discord', ($in['msg_id'] ?? null) !== null ? (string)$in['msg_id'] : null,
            ($in['posted_at'] ?? null) !== null ? (string)$in['posted_at'] : null,
            $status,
            $gid ? ('matched by ' . $how) : ($why ?: 'could not place it'));
        $queued++;
    }
    /* A clip in a fixture's own thread means that fixture has been played, so the
       thread is marked FINAL. Only for a thread THIS SITE created and only when the
       series is actually complete - a clip of game 1 of a best-of-three is not the
       end of anything. Failure is reported, never fatal: the footage is already
       filed and a title is cosmetic beside that. */
    $finalDid = null;
    if ($how === 'thread' && $gid > 0 && $queued > 0) {
        $done = false;
        foreach (nsr_series($pdo, $leagueId) as $s) {
            if ((int)$s['game_id'] === $gid) { $done = !empty($s['complete']); break; }
        }
        if ($done) {
            require_once __DIR__ . '/../_inc/nsr_threadpost.php';
            $r = nsr_thread_mark_final($pdo, $leagueId, $gid);
            $finalDid = (string)$r['did'];
        }
    }

    out(['ok' => true, 'queued' => $queued, 'league_id' => $leagueId,
         'game_id' => $gid ?: null, 'series_game_no' => $sg,
         'matched_by' => $how, 'reason' => $why, 'status' => $status,
         'final' => $finalDid]);
}

/* ---------------------------------------------------------------- context -- */
/**
 * FACTS for a question, so a model can answer it in its own words.
 *
 * THE POINT: the structured path answers a fixed menu - leaders, one player's
 * stat, a club's record - and anything phrased outside it fell through. This
 * hands over the relevant numbers instead and lets the model write the sentence,
 * which covers "how good was he", "compare these two", "who should I worry about"
 * and everything else nobody enumerated.
 *
 * The model still never invents a figure: it only ever sees this JSON, and it is
 * told to answer from it or say it cannot. Every number here comes from the same
 * helpers the site's own pages use.
 */
if ($action === 'context') {
    if ($leagueId <= 0) fail('league_id required');
    require_once __DIR__ . '/bot_stat.php';

    $all      = !empty($in['all_seasons']);
    $pName    = trim((string)($in['player'] ?? ''));
    $tName    = trim((string)($in['team'] ?? ''));
    $week     = (int)($in['week'] ?? 0);
    $metric   = (string)($in['metric'] ?? '');

    $seasons  = nsr_seasons($pdo);

    /* FIND THE RIGHT SEASON FROM THE NAME, when the question did not give one.
       "Adam's record with the Subway Surfers" was answered against whatever league
       the caller defaulted to - Creation - where no such club exists, so the reply
       was "no data" about a club with a full season of it. A club or player named
       in the question is itself the strongest clue about which season is meant. */
    /* A season named in the question routes the whole request, exactly as it
       does for stat_query. Without this, "Xavier Worthy in Reborn Season 1" was
       answered from whatever league the caller defaulted to. */
    $lgName = trim((string)($in['league'] ?? ''));
    if ($lgName !== '' && strcasecmp($lgName, 'all') !== 0) {
        foreach ($seasons as $id => $L) {
            $nm = (string)($L['season'] ?? '');
            if ((string)$id === $lgName || ($nm !== '' && strcasecmp($nm, $lgName) === 0)) {
                $leagueId = (int)$id;
                break;
            }
        }
    }

    if (empty($in['league_named']) && !$all) {
        $probe = $tName !== '' ? $tName : $pName;
        if ($probe !== '') {
            $found = 0;
            foreach ($seasons as $lid => $L) {
                $lid = (int)$lid;
                $hit = $tName !== ''
                    ? bot_stat_team($pdo, $lid, $probe)
                    : bot_stat_player($pdo, $lid, $probe);
                if ($hit) { $found = $lid; if ($lid === $leagueId) break; }
            }
            if ($found > 0) $leagueId = $found;
        }
    }

    $ctx = ['season' => (string)($seasons[$leagueId]['season'] ?? ('League ' . $leagueId))];

    /* ---- an OWNER, which is a person, not a club ------------------------- */
    $oName = trim((string)($in['owner'] ?? ''));
    if ($oName !== '') {
        require_once __DIR__ . '/../_inc/nsr_owners.php';
        $norm = static fn(string $x): string => preg_replace('/[^a-z0-9]/', '', mb_strtolower($x));
        $want = $norm($oName);
        foreach (nsr_owners($pdo) as $o) {
            $nm = (string)($o['name'] ?? $o['handle'] ?? '');
            if ($nm === '' || $norm($nm) !== $want) continue;
            $row = ['name' => $nm,
                    'championships' => (int)($o['titles'] ?? $o['champion'] ?? 0),
                    'playoff_appearances' => (int)($o['playoffs'] ?? 0),
                    'seasons' => []];
            foreach ((array)($o['seasons'] ?? []) as $sn) {
                $row['seasons'][] = [
                    'season' => (string)($sn['season'] ?? ''),
                    'club'   => (string)($sn['team'] ?? ''),
                    'record' => (int)($sn['w'] ?? 0) . '-' . (int)($sn['l'] ?? 0),
                    'seed'   => (int)($sn['seed'] ?? 0) ?: null,
                    'result' => !empty($sn['champion']) ? 'won the title'
                              : (!empty($sn['final']) ? 'lost the final'
                              : (!empty($sn['playoffs']) ? 'made the playoffs'
                              : (empty($sn['over']) ? 'in progress' : 'missed the playoffs'))),
                ];
            }
            $ctx['owner'] = $row;
            break;
        }
    }

    /* ---- a named player -------------------------------------------------- */
    if ($pName !== '') {
        if ($all) {
            $hit = bot_stat_alltime_player($pdo, $pName);
            if ($hit && empty($hit['ambiguous'])) {
                $row = ['name' => (string)$hit['name'], 'seasons' => (int)($hit['seasons'] ?? 0)];
                foreach (bot_stat_vocab() as $k => $_) {
                    if (!empty($hit[$k])) $row[$k] = (int)$hit[$k];
                }
                $ctx['player_career'] = $row;
            } elseif ($hit) {
                $ctx['player_ambiguous'] = array_map(fn($p) => (string)$p['name'], $hit['ambiguous']);
            }
        } else {
            $hit = bot_stat_player($pdo, $leagueId, $pName);
            if ($hit && empty($hit['ambiguous'])) {
                $rows = nsr_player_stat_rows($pdo, $leagueId, $week > 0 ? $week : null);
                $r = $rows[(int)$hit['poid']] ?? [];
                $teams = nsr_teams($pdo, $leagueId);
                $row = ['name' => (string)$hit['name'],
                        'team' => (string)($teams[(int)($r['team_id'] ?? 0)]['name'] ?? ''),
                        'games' => (int)($r['gp'] ?? 0), 'series' => (int)($r['ser'] ?? 0)];
                foreach (bot_stat_vocab() as $k => $_) if (!empty($r[$k])) $row[$k] = (int)$r[$k];
                /* The card the bot embeds under a statline answer. Built here so
                   the poid never has to leave the site's own vocabulary. */
                $row['statbox'] = '/league/og/statbox.php?league_id=' . $leagueId
                                . '&poid=' . (int)$hit['poid'];
                $ctx['player'] = $row;
                /* Where he ranks, which is what "how good was he" actually asks. */
                $keys = array_slice(array_keys(array_filter($r, fn($v, $k) =>
                    is_int($v) && $v > 0 && array_key_exists($k, bot_stat_vocab()), ARRAY_FILTER_USE_BOTH)), 0, 6);
                if ($keys) $ctx['player_ranks'] = nsr_player_ranks($pdo, $leagueId, (int)$hit['poid'], $keys);
            } elseif ($hit) {
                $ctx['player_ambiguous'] = array_map(fn($p) => (string)$p['name'], $hit['ambiguous']);
            }
        }
    }

    /* ---- a SECOND player, so comparisons work ----------------------------
       "compare Geno and Lamar" used to answer on whichever name was extracted
       first and then say the other's numbers were missing - true, and useless.
       The second slot is filled the same way as the first. */
    $p2Name = trim((string)($in['player2'] ?? ''));
    if ($p2Name !== '') {
        $hit2 = $all ? bot_stat_alltime_player($pdo, $p2Name)
                     : bot_stat_player($pdo, $leagueId, $p2Name);
        if ($hit2 && empty($hit2['ambiguous'])) {
            if ($all) {
                $row = ['name' => (string)$hit2['name'], 'seasons' => (int)($hit2['seasons'] ?? 0)];
                foreach (bot_stat_vocab() as $k => $_) if (!empty($hit2[$k])) $row[$k] = (int)$hit2[$k];
                $ctx['player2_career'] = $row;
            } else {
                $rows2 = nsr_player_stat_rows($pdo, $leagueId, $week > 0 ? $week : null);
                $r2 = $rows2[(int)$hit2['poid']] ?? [];
                $teams2 = nsr_teams($pdo, $leagueId);
                $row = ['name' => (string)$hit2['name'],
                        'team' => (string)($teams2[(int)($r2['team_id'] ?? 0)]['name'] ?? ''),
                        'games' => (int)($r2['gp'] ?? 0)];
                foreach (bot_stat_vocab() as $k => $_) if (!empty($r2[$k])) $row[$k] = (int)$r2[$k];
                $ctx['player2'] = $row;
            }
        }
    }

    /* ---- a named club ---------------------------------------------------- */
    if ($tName !== '') {
        $t = bot_stat_team($pdo, $leagueId, $tName);
        if ($t) {
            $srows = nsr_standings_rows($pdo, $leagueId)['rows'];
            foreach ($srows as $i => $row) {
                if ((int)$row['id'] !== (int)$t['id']) continue;
                $ctx['team'] = ['name' => (string)$t['name'], 'wins' => (int)$row['w'],
                                'losses' => (int)$row['l'], 'position' => $i + 1,
                                'of' => count($srows)];
                break;
            }
            $tr = nsr_team_stat_rows($pdo, $leagueId, $week > 0 ? $week : null);
            $tt = $tr[(int)$t['id']] ?? [];
            $line = [];
            foreach (bot_stat_vocab() as $k => $_) if (!empty($tt[$k])) $line[$k] = (int)$tt[$k];
            if ($line) $ctx['team_totals'] = $line;
        }
    }

    /* ---- leaders, for the metric asked about or a sensible default ------- */
    $keys = $metric !== '' && array_key_exists($metric, bot_stat_vocab())
        ? [$metric]
        : ['pass_td', 'rush_td', 'rec_td', 'sack', 'int', 'tackle'];
    $rows  = $all ? nsr_alltime_player_rows($pdo)
                  : nsr_player_stat_rows($pdo, $leagueId, $week > 0 ? $week : null);
    $names = $all ? null : nsr_player_names($pdo, $leagueId);
    $lead = [];
    foreach ($keys as $k) {
        $top = [];
        foreach (nsr_rank_rows($rows, $k, 5) as $e) {
            $r = $e['row'];
            $nm = $all ? (string)($r['name'] ?? '?') : (string)($names[(int)$r['poid']] ?? '?');
            $top[] = [$nm, (int)$r[$k]];
        }
        if ($top) $lead[$k] = $top;
    }
    if ($lead) $ctx['leaders'] = $lead;

    /* ---- the table, always: most questions lean on it somewhere ---------- */
    if (!$all) {
        $tbl = [];
        foreach (array_slice(nsr_standings_rows($pdo, $leagueId)['rows'], 0, 8) as $i => $r) {
            $tbl[] = [$i + 1, (string)$r['name'], (int)$r['w'], (int)$r['l']];
        }
        if ($tbl) $ctx['standings'] = $tbl;
    }

    /* ---- head to head, when two clubs are named --------------------------- */
    $t2Name = trim((string)($in['team2'] ?? ''));
    if ($tName !== '' && $t2Name !== '') {
        $a = bot_stat_team($pdo, $leagueId, $tName);
        $b = bot_stat_team($pdo, $leagueId, $t2Name);
        if ($a && $b && (int)$a['id'] !== (int)$b['id']) {
            $meet = [];
            foreach (nsr_series($pdo, $leagueId) as $sx) {
                $h = (int)$sx['home_team_id']; $aw = (int)$sx['away_team_id'];
                $pair = [$h, $aw];
                if (!in_array((int)$a['id'], $pair, true) || !in_array((int)$b['id'], $pair, true)) continue;
                $meet[] = [
                    'week'   => nsr_week_label($leagueId, (int)$sx['week_num']),
                    'home'   => (string)(nsr_teams($pdo, $leagueId)[$h]['name'] ?? ''),
                    'away'   => (string)(nsr_teams($pdo, $leagueId)[$aw]['name'] ?? ''),
                    'series' => (int)$sx['away_wins'] . '-' . (int)$sx['home_wins'],
                    'played' => !empty($sx['complete']),
                ];
            }
            if ($meet) $ctx['head_to_head'] = $meet;
        }
    }

    /* ---- the bracket, and who has won what --------------------------------
       "can we talk about playoff results" needs the rounds themselves, not just a
       champion: which clubs were in, what each series finished, and the round it
       belonged to. */
    require_once __DIR__ . '/../_inc/playoff_config.php';
    $cfg = get_playoff_config($pdo, $leagueId);
    $first = playoff_first_week($cfg);
    if ($first > 0) {
        $po = [];
        $teamsAll = nsr_teams($pdo, $leagueId);
        foreach (nsr_series($pdo, $leagueId) as $sx) {
            if ((int)$sx['week_num'] < $first) continue;
            $po[] = [
                'round' => nsr_week_label($leagueId, (int)$sx['week_num']),
                'away'  => (string)($teamsAll[(int)$sx['away_team_id']]['name'] ?? ''),
                'home'  => (string)($teamsAll[(int)$sx['home_team_id']]['name'] ?? ''),
                'series'=> (int)$sx['away_wins'] . '-' . (int)$sx['home_wins'],
                'played'=> !empty($sx['complete']),
            ];
        }
        if ($po) $ctx['playoffs'] = $po;
    }

    require_once __DIR__ . '/../_inc/nsr_owners.php';
    $champs = nsr_champions($pdo);
    if (!empty($champs[$leagueId])) {
        $tt = nsr_teams($pdo, $leagueId);
        $ctx['champion'] = (string)($tt[(int)$champs[$leagueId]]['name'] ?? '');
    }
    if ($all) {
        $hist = [];
        foreach (nsr_seasons($pdo) as $lid => $L) {
            if (empty($champs[$lid])) continue;
            $tt = nsr_teams($pdo, (int)$lid);
            if (!isset($tt[(int)$champs[$lid]])) continue;
            $hist[(string)($L['season'] ?? $lid)] = (string)$tt[(int)$champs[$lid]]['name'];
        }
        if ($hist) $ctx['champions_by_season'] = $hist;
    }

    /* ---- the SAME player in a SECOND season, for cross-season comparison -- */
    $lg2Name = trim((string)($in['league2'] ?? ''));
    if ($lg2Name !== '' && $pName !== '') {
        $lid2 = 0;
        foreach ($seasons as $id => $L) {
            $nm = (string)($L['season'] ?? '');
            if ((string)$id === $lg2Name || ($nm !== '' && strcasecmp($nm, $lg2Name) === 0)) { $lid2 = (int)$id; break; }
        }
        if ($lid2 > 0 && $lid2 !== $leagueId) {
            $hit = bot_stat_player($pdo, $lid2, $pName);
            if ($hit && empty($hit['ambiguous'])) {
                $rowsX = nsr_player_stat_rows($pdo, $lid2);
                $rX = $rowsX[(int)$hit['poid']] ?? [];
                $teamsX = nsr_teams($pdo, $lid2);
                $row = ['season' => (string)($seasons[$lid2]['season'] ?? $lid2),
                        'name' => (string)$hit['name'],
                        'team' => (string)($teamsX[(int)($rX['team_id'] ?? 0)]['name'] ?? '')];
                foreach (bot_stat_vocab() as $k => $_) if (!empty($rX[$k])) $row[$k] = (int)$rX[$k];
                $ctx['player_other_season'] = $row;
            }
        }
    }

    /* ---- topical sections, included when the question touches them --------
       Bounded on purpose: the model reads all of this on every answer, so each
       section is capped rather than complete. */
    $topics = $in['topics'] ?? [];
    if (is_string($topics)) $topics = array_filter(array_map('trim', explode(',', $topics)));
    $topics = array_map('strval', (array)$topics);
    $teamCtxId = 0;
    if ($tName !== '') { $tt = bot_stat_team($pdo, $leagueId, $tName); $teamCtxId = (int)($tt['id'] ?? 0); }

    /* Roster: cheap, and answers "who plays for X". */
    if ($teamCtxId > 0) {
        $q = $pdo->prepare(
            "SELECT lpr.poid FROM league_player_rights lpr
              WHERE lpr.league_id = ? AND lpr.team_id = ? AND lpr.status = 'rostered'");
        $q->execute([$leagueId, $teamCtxId]);
        $namesR = nsr_player_names($pdo, $leagueId);
        $ros = [];
        foreach ($q->fetchAll(PDO::FETCH_COLUMN) as $poid) {
            $n = (string)($namesR[(int)$poid] ?? '');
            if ($n !== '') $ros[] = $n;
        }
        if ($ros) $ctx['roster'] = $ros;
    }

    /* Results: one week's scorelines, or a named club's whole season. */
    if ($week > 0 || $teamCtxId > 0) {
        $teamsAll2 = nsr_teams($pdo, $leagueId);
        $res = [];
        foreach (nsr_series($pdo, $leagueId, $week > 0 ? $week : null) as $sx) {
            if ($teamCtxId > 0 && (int)$sx['home_team_id'] !== $teamCtxId
                && (int)$sx['away_team_id'] !== $teamCtxId) continue;
            $res[] = [
                'week'   => nsr_week_label($leagueId, (int)$sx['week_num']),
                'away'   => (string)($teamsAll2[(int)$sx['away_team_id']]['name'] ?? ''),
                'home'   => (string)($teamsAll2[(int)$sx['home_team_id']]['name'] ?? ''),
                'series' => (int)$sx['away_wins'] . '-' . (int)$sx['home_wins'],
                'played' => !empty($sx['complete']),
            ];
            if (count($res) >= 24) break;
        }
        if ($res) $ctx['results'] = $res;
    }

    /* Draft: the named club's picks, or the opening of the board. */
    if (in_array('draft', $topics, true)) {
        $namesD = nsr_player_names($pdo, $leagueId);
        $teamsD = nsr_teams($pdo, $leagueId);
        $sql = 'SELECT team_id, poid, round_num, pick_num FROM draft_picks WHERE league_id = ?';
        $args = [$leagueId];
        if ($teamCtxId > 0) { $sql .= ' AND team_id = ?'; $args[] = $teamCtxId; }
        $sql .= ' ORDER BY pick_num LIMIT 32';
        $q = $pdo->prepare($sql);
        $q->execute($args);
        $dr = [];
        foreach ($q->fetchAll(PDO::FETCH_ASSOC) as $d) {
            $dr[] = ['pick' => (int)$d['pick_num'], 'round' => (int)$d['round_num'],
                     'club' => (string)($teamsD[(int)$d['team_id']]['name'] ?? ''),
                     'player' => (string)($namesD[(int)$d['poid']] ?? ('#' . $d['poid']))];
        }
        if ($dr) $ctx['draft'] = $dr;
    }

    /* Awards: the season's winners, and the named player's cabinet. */
    if (in_array('awards', $topics, true)) {
        require_once __DIR__ . '/../_inc/awards_helper.php';
        try {
            awards_seed_if_empty($pdo, $leagueId);
            $aw = [];
            $cat = $pdo->prepare('SELECT * FROM league_awards WHERE league_id = ? LIMIT 12');
            $cat->execute([$leagueId]);
            foreach ($cat->fetchAll(PDO::FETCH_ASSOC) as $a) {
                $w = awards_winner($pdo, $leagueId, $a);
                if ($w) $aw[(string)($a['name'] ?? $a['award_key'] ?? '?')] = (string)($w['name'] ?? '?');
            }
            if ($aw) $ctx['awards'] = $aw;
            if ($pName !== '') {
                $mine = awards_for_player($pdo, $pName);
                if ($mine) {
                    $ctx['player_awards'] = array_map(
                        static fn(array $x): string => (string)($x['label'] ?? $x['name'] ?? '?'), $mine);
                }
            }
        } catch (Throwable $e) { /* awards not set up for this league */ }
    }

    /* Transactions: the newest moves, names resolved. */
    if (in_array('transactions', $topics, true)) {
        $namesT = nsr_player_names($pdo, $leagueId);
        $teamsT = nsr_teams($pdo, $leagueId);
        $q = $pdo->prepare(
            'SELECT t.created_at, i.action, i.poid, i.from_team_id, i.to_team_id
               FROM transaction_items i JOIN transactions t ON t.id = i.transaction_id
              WHERE i.league_id = ? ORDER BY i.id DESC LIMIT 25');
        $q->execute([$leagueId]);
        $tx = [];
        foreach ($q->fetchAll(PDO::FETCH_ASSOC) as $x) {
            $tx[] = [
                'date'   => substr((string)$x['created_at'], 0, 10),
                'action' => (string)$x['action'],
                'player' => (string)($namesT[(int)$x['poid']] ?? ('#' . $x['poid'])),
                'from'   => $x['from_team_id'] ? (string)($teamsT[(int)$x['from_team_id']]['name'] ?? '') : null,
                'to'     => $x['to_team_id'] ? (string)($teamsT[(int)$x['to_team_id']]['name'] ?? '') : null,
            ];
        }
        if ($tx) $ctx['transactions'] = $tx;
    }

    out(['ok' => true, 'context' => $ctx]);
}

/* -------------------------------------------------------------- post_week -- */

/**
 * Post the week's game threads. This is the Wednesday job, reachable over HTTP so
 * a scheduler anywhere - Railway's cron, for instance - can fire it without the
 * web host needing cron of its own and without anything but this token.
 *
 * The rule lives in PHP, not in the caller: it posts the earliest week that still
 * has fixtures without threads UNLESS that week is already finished and
 * stat-recorded. A caller asking for a specific week gets that week.
 */
if ($action === 'post_week') {
    require_once __DIR__ . '/../_inc/nsr_threadpost.php';
    $go   = !empty($in['go']);
    $week = (int)($in['week'] ?? 0);

    /* TWO TRIGGERS, told apart by this flag.
       The hourly check sends only_if_done: it may post ONLY when every earlier
       week has finished by that league's own rule - stats for Creation, footage
       for Franchise. The Friday run sends nothing and posts whatever is next.
       This assignment previously sat in the `fixtures` handler by mistake, so in
       here it was undefined, !empty(null) was false, and the gate below never ran
       once - which is how an hourly tick posted a week nobody had finished. */
    $onlyIfDone = !empty($in['only_if_done']);

    $leagues = $leagueId > 0 ? [$leagueId] : array_keys(NSR_THREAD_FORUMS);
    $out = [];
    foreach ($leagues as $lid) {
        if (nsr_thread_forum((int)$lid) === '') {
            $out[] = ['league_id' => (int)$lid, 'skipped' => 'no forum configured'];
            continue;
        }
        $plan = nsr_thread_plan($pdo, (int)$lid);
        $w = $week;
        if ($w <= 0) foreach ($plan as $k => $p) if ($p['due']) { $w = (int)$k; break; }

        if ($w <= 0) {
            $out[] = ['league_id' => (int)$lid, 'skipped' => 'nothing due'];
            continue;
        }
        if ($week <= 0 && !empty($plan[$w]['done'])) {
            $out[] = ['league_id' => (int)$lid, 'week' => $w,
                      'skipped' => 'already played out in full'];
            continue;
        }

                    /* The early-post rule: only when EVERY earlier week is finished. Asking about
                       the week before the due one is not enough on its own - a gap further back
                       would let a mid-season week post the moment one week completed. */
                    if ($onlyIfDone) {
                        $allPriorDone = true;
                        $priorWeeks   = 0;
                        foreach ($plan as $k => $pw) {
                            if ((int)$k >= $w) break;
                            if ((int)($pw['fixtures'] ?? 0) <= 0) continue;
                            $priorWeeks++;
                            if (empty($pw['done'])) { $allPriorDone = false; break; }
                        }
                        /* THE FIRST WEEK HAS NOTHING BEHIND IT, so "every earlier week is
                           finished" is vacuously true and the hourly check happily posted
                           it - which is how week 1 went up twice. The completion trigger
                           needs a completed week to be triggered BY; opening week is the
                           scheduled run's job. */
                        if ($priorWeeks === 0) {
                            $out[] = ['league_id' => (int)$lid, 'week' => $w,
                                      'skipped' => 'first week - waits for the scheduled run'];
                            continue;
                        }
                        if (!$allPriorDone) {
                            $out[] = ['league_id' => (int)$lid, 'week' => $w,
                                      'skipped' => 'an earlier week is not finished yet'];
                            continue;
                        }
                    }
        $r = nsr_thread_post($pdo, (int)$lid, $w, $go);
        $out[] = ['league_id' => (int)$lid, 'week' => $w, 'go' => $go,
                  'made' => count($r['made']), 'msg' => $r['msg']];
    }
    out(['ok' => true, 'dry' => !$go, 'leagues' => $out]);
}

/* ------------------------------------------------------------------- stat -- */

if ($action === 'stat') {
    if ($leagueId <= 0) fail('league_id required');
    $q = trim((string)($in['q'] ?? ''));
    if ($q === '') fail('q required');
    require_once __DIR__ . '/bot_stat.php';
    out(bot_stat_answer($pdo, $leagueId, $q));
}

/**
 * What a question may be translated INTO.
 *
 * The caller needs this to instruct its model, and the list has exactly one
 * home - here, derived from bot_stat_vocab() - so the two cannot drift. A
 * hardcoded copy in the bot would go stale the first time a stat key is added,
 * and the symptom would be the model confidently naming a metric this endpoint
 * then rejects.
 */
if ($action === 'stat_schema') {
    require_once __DIR__ . '/bot_stat.php';
    $metrics = [];
    foreach (bot_stat_vocab() as $key => $words) {
        $metrics[$key] = $words;          // the words people actually use for it
    }
    /* The seasons, so a question that NAMES one can be honoured. Without this the
       model had no field to put "Reborn Season 3" in, so it dropped it and the
       answer came from whichever league the caller defaulted to - the right
       statistic from the wrong season, which reads as simply wrong. */
    /* Only seasons that have actually been played. "Test League" exists with 18
       clubs, 0 games and 0 stat events; offering it as a choice invites the model
       to answer a question about it, and every answer would be an empty one. */
    $played = $pdo->query(
        'SELECT league_id FROM games GROUP BY league_id
         UNION SELECT league_id FROM stat_events GROUP BY league_id')->fetchAll(PDO::FETCH_COLUMN);
    $played = array_map('intval', $played);
    $leagues = [];
    foreach (nsr_seasons($pdo) as $id => $s) {
        if (!in_array((int)$id, $played, true)) continue;
        $leagues[(string)$id] = (string)($s['season'] ?? ('League ' . $id));
    }
    out(['ok' => true,
         'metrics' => $metrics,
         'leagues' => $leagues,
         'intents' => ['leaders', 'player_stat', 'player_line', 'team_stat',
                       'team_record', 'standings', 'champion'],
         'scopes'  => ['season', 'week']]);
}

/**
 * The same answer, from a question a language model has already parsed.
 *
 * The caller sends both the structured reading AND the original words. If the
 * reading turns out to be unusable - a metric this league does not store, a name
 * that matches nobody - the words are still here, so it falls back to the keyword
 * matcher instead of returning nothing. That makes a bad translation no worse
 * than having no model at all.
 *
 * The figures are computed HERE either way. The model's only influence is on which
 * question gets asked of the database, never on the answer.
 */
if ($action === 'stat_query') {
    if ($leagueId <= 0) fail('league_id required');
    $q  = trim((string)($in['q'] ?? ''));
    $sq = $in['query'] ?? null;
    if (!is_array($sq)) fail('query object required');
    require_once __DIR__ . '/bot_stat.php';

    /* A season named IN the question wins over the one the caller defaulted to.
       Asking "who led rushing tds in Reborn Season 3" from a client whose default
       is Creation League should answer about Season 3 - the words are explicit and
       the default is only a fallback. Only a league that exists is accepted, so a
       misheard name falls back rather than reaching for a league id that is not
       there. (Note this is the opposite of the footage rule, where the thread's
       league is authoritative and a disagreement is refused: there the mapping is
       a recorded fact, here the user has just said which season they mean.) */
    $want = trim((string)($sq['league'] ?? ''));
    if ($want !== '' && strcasecmp($want, 'all') !== 0) {
        $hit = false;
        foreach (nsr_seasons($pdo) as $id => $s) {
            $nm = (string)($s['season'] ?? '');
            if ((string)$id === $want
                || ($nm !== '' && strcasecmp($nm, $want) === 0)) {
                $leagueId = (int)$id; $hit = true; break;
            }
        }
        /* A season name that matches nothing is treated as no season at all, which
           means the whole history - a fuller answer than silently using the
           caller's default, and one that names its own scope. */
        if (!$hit) $want = 'all';
    }

    /* NO SEASON NAMED MEANS EVERY SEASON. Somebody asking "who has the most
       sacks" has asked about the league; answering from whichever season the
       client defaulted to was indistinguishable from answering wrongly. Every
       all-time reply states how many seasons it covers.

       EXCEPT for the questions that only exist inside one season. A table, a
       club's record and a club's totals have no all-time meaning, and answering
       "name a season" to "what are the standings" is precisely the friction this
       is meant to remove - so those quietly use the CURRENT season, which is the
       first one the site lists that has actually been played. */
    if ($want === '' || strcasecmp($want, 'all') === 0) {
        $perSeason = in_array((string)($sq['intent'] ?? ''),
                              ['standings', 'team_record', 'team_stat'], true)
                     || !empty($sq['team']);
        if ($perSeason) {
            $chosen = 0;
            /* A club names its own season. "How are the Playzers doing" answered
               `unknown` because the Playzers play in Reborn 1 and 2 and the
               fallback looked in the current season, where no such club exists.
               So when a club is named, the season used is the most recent one
               that actually fielded it. */
            if (!empty($sq['team'])) {
                foreach (nsr_seasons($pdo) as $id => $s) {
                    if (bot_stat_team($pdo, (int)$id, (string)$sq['team'])) {
                        $chosen = (int)$id; break;
                    }
                }
            }
            /* Otherwise the current season: the first the site lists that has
               actually been played. */
            if (!$chosen) {
                foreach (nsr_seasons($pdo) as $id => $s) {
                    $c = $pdo->prepare('SELECT 1 FROM games WHERE league_id = ? LIMIT 1');
                    $c->execute([$id]);
                    if ($c->fetchColumn()) { $chosen = (int)$id; break; }
                }
            }
            if ($chosen) $leagueId = $chosen;
            unset($sq['league']);
        } else {
            $sq['league'] = 'all';
        }
    }

    /* A DELIBERATELY absent metric is an answer, not a failure.
       The model is told to return no metric when the question asks for something
       this league does not record, and asking for a stat that does not exist is
       the commonest such question - there is no yardage anywhere in stat_events.
       Falling back to the keyword matcher here actively destroys that honesty:
       asked "who threw the most passing yards" it matched the word "passing",
       answered with passing TOUCHDOWNS, and titled it "Passing leaders" - a
       different statistic presented as the one requested. So when the reading
       names a stat-shaped intent but no metric, say so and stop. */
    $needsMetric = in_array((string)($sq['intent'] ?? ''),
                            ['leaders', 'player_stat', 'team_stat'], true);
    if ($needsMetric && empty($sq['metric'])) {
        out(['ok' => false, 'kind' => 'untracked_metric', 'via' => 'model',
             'answer' => "That is not something the league records.\n"
                       . 'Recorded categories: '
                       . implode(', ', array_map(
                             static fn(string $k): string => nsr_stat_word($k, 'short'),
                             array_keys(bot_stat_vocab())))
                       . '.']);
    }

    $r = bot_stat_answer($pdo, $leagueId, $q, $sq);
    /* A translation that led nowhere still has the original wording to fall back
       on - worth trying, because the two read the question differently. */
    if (empty($r['ok']) && ($r['kind'] ?? '') === 'unknown' && $q !== '') {
        $alt = bot_stat_answer($pdo, $leagueId, $q);
        if (!empty($alt['ok'])) { $alt['via'] = 'keywords'; out($alt); }
    }
    $r['via'] = 'model';
    out($r);
}

/* -------------------------------------------------------------- team_home -- */
/**
 * One member's club, for the Reborn launcher's League tab.
 *
 * Called by the Reborn backend, never by the launcher: the backend knows who is
 * signed in and passes that person's Discord id. Someone with no club in the
 * current league gets member:false. See _inc/nsr_team_home.php for the shape.
 */
if ($action === 'team_home') {
    require_once __DIR__ . '/../_inc/nsr_team_home.php';
    try {
        $home = nsr_team_home($pdo, (string)($in['discord_user_id'] ?? ''));
    } catch (InvalidArgumentException $e) {
        fail($e->getMessage());
    }
    /* A player name with a stray byte in the pool would make json_encode return
       false and the backend read an empty body; substitute instead. */
    echo json_encode(['ok' => true] + $home, JSON_INVALID_UTF8_SUBSTITUTE);
    exit;
}

fail('Unknown action: ' . $action);
