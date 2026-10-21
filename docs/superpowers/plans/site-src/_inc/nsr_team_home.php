<?php
declare(strict_types=1);

/**
 * One club's home page as data, for the Reborn launcher's League tab.
 *
 * The caller is the Reborn backend, never the launcher: it knows who is signed
 * in and sends that person's Discord id. The id is matched to league_owners and
 * from there to the club that person runs in the CURRENT league - the newest one
 * not hidden. A club in an older season does not count, so a former member gets
 * {member:false} rather than a finished season dressed up as this week.
 *
 * Read-only. Every figure comes from the helpers the site's own pages use, so the
 * tab and the website cannot disagree about a record or a table.
 */

require_once __DIR__ . '/nsr_data.php';
require_once __DIR__ . '/nsr_standings.php';

/** Where the site is served, for absolute logo and page links. */
const NSR_PUBLIC_ORIGIN = 'https://nflstreetreborn.com';

/** A site path as an absolute URL; null when there is no path at all. */
function nsr_th_url(?string $path): ?string {
    $p = trim((string)$path);
    if ($p === '') return null;
    if (preg_match('~^https?://~i', $p)) return $p;
    return NSR_PUBLIC_ORIGIN . '/' . ltrim($p, '/');
}

/** The league the tab is about: the newest one not hidden. 0 when there is none. */
function nsr_th_current_league(PDO $pdo): int {
    $ids = array_map('intval', $pdo->query('SELECT id FROM leagues ORDER BY id DESC')->fetchAll(PDO::FETCH_COLUMN));
    foreach ($ids as $id) {
        if (!in_array($id, NSR_HIDDEN_LEAGUES, true)) return $id;
    }
    return 0;
}

/**
 * The week the tab is about.
 *
 * The site's own current week stays on a week once it is fully played - right
 * for a masthead showing last week's finals, wrong for "who do I play this
 * week". So when every series in that week is settled, move on to the next week
 * that has fixtures. A finished season stays on its last week.
 */
function nsr_th_week(PDO $pdo, int $leagueId): int {
    $w = nsr_current_week($pdo, $leagueId);
    foreach (nsr_series($pdo, $leagueId, $w) as $s) {
        if (!$s['complete']) return $w;
    }
    foreach (nsr_weeks($pdo, $leagueId) as $n) {
        if ($n > $w) return $n;
    }
    return $w;
}

/**
 * The fixture a club's matchup card is about, chosen per club.
 *
 * One league-wide week is wrong for this: one postponed fixture holds the whole
 * league's week back and freezes everyone else's card on a game they already
 * finished. So each club gets its own earliest series that is NOT complete, in
 * any week from the site's current week on; failing that, its series in the
 * tab's week (which may be final); failing that, null = a bye.
 *
 * $all is nsr_series() for the whole league (ordered by week, then id).
 */
function nsr_th_pick_matchup(array $all, int $teamId, int $floor, int $week): ?array {
    $inWeek = null;
    foreach ($all as $s) {
        if ((int)$s['home_team_id'] !== $teamId && (int)$s['away_team_id'] !== $teamId) continue;
        if (!$s['complete'] && (int)$s['week_num'] >= $floor) return $s;
        if ($inWeek === null && (int)$s['week_num'] === $week) $inWeek = $s;
    }
    return $inWeek;
}

/** One side of a fixture, as the scoreboard draws it. */
function nsr_th_side(array $t, array $records): array {
    return [
        'team_id'  => (int)$t['id'],
        'name'     => (string)$t['name'],
        'abbr'     => (string)($t['abbrev'] ?? ''),
        'logo_url' => nsr_th_url($t['logo_path'] ?? null),
        'color'    => (string)($t['primary_color'] ?? ''),
        'record'   => $records[(int)$t['id']] ?? '0-0',
    ];
}

function nsr_th_status(array $s): string {
    if (!empty($s['complete'])) return 'final';
    if ((int)$s['played'] > 0)  return 'in_progress';
    return 'scheduled';
}

/** Summed game points, once anything has been played. */
function nsr_th_score(array $s): ?array {
    if ((int)$s['played'] === 0) return null;
    return ['home' => (int)$s['home_points'], 'away' => (int)$s['away_points']];
}

/** Games won in the series, once anything has been played or a winner recorded. */
function nsr_th_series(array $s): ?array {
    if ((int)$s['played'] === 0 && nsr_series_winner($s) === null) return null;
    return ['home' => (int)$s['home_wins'], 'away' => (int)$s['away_wins']];
}

/**
 * team_id => owner display name and Discord id, for one league.
 *
 * Should a club ever carry more than one owner row, the answer must not depend
 * on row order: owners with no Discord id sort first and linked owners last, and
 * the loop keeps the LAST row per club, so a linked owner always wins.
 */
function nsr_th_owners(PDO $pdo, int $leagueId): array {
    $st = $pdo->prepare(
        "SELECT ot.team_id, o.handle, o.display_name, o.discord_user_id
           FROM owner_teams ot JOIN league_owners o ON o.id = ot.owner_id
          WHERE ot.league_id = ?
          ORDER BY (o.discord_user_id IS NULL OR o.discord_user_id = '') DESC, o.id DESC");
    $st->execute([$leagueId]);
    $out = [];
    foreach ($st->fetchAll(PDO::FETCH_ASSOC) as $r) {   // last row per club wins
        $d = trim((string)($r['discord_user_id'] ?? ''));
        $out[(int)$r['team_id']] = [
            'display' => (string)($r['display_name'] ?: $r['handle']),
            'discord' => $d !== '' ? $d : null,
        ];
    }
    return $out;
}

/** poid => jersey number (pool field PJEN), for the league's pinned pool. */
function nsr_th_numbers(PDO $pdo, int $leagueId): array {
    $st = $pdo->prepare("SELECT pool_version_id FROM leagues WHERE id = ?");
    $st->execute([$leagueId]);
    $vid = (int)$st->fetchColumn();
    if ($vid <= 0) return [];
    $q = $pdo->prepare("SELECT poid, JSON_UNQUOTE(JSON_EXTRACT(row_json, '$.PJEN')) AS n
                          FROM pool_csv_rows WHERE version_id = ?");
    $q->execute([$vid]);
    $out = [];
    foreach ($q->fetchAll(PDO::FETCH_ASSOC) as $r) {
        if ($r['n'] !== null && $r['n'] !== '' && is_numeric($r['n'])) $out[(int)$r['poid']] = (int)$r['n'];
    }
    return $out;
}

/** The rest of the page: scoreboard, table, results, leaders, roster. */
function nsr_th_page(PDO $pdo, int $lid, int $teamId, array $teams, array $records, int $week): array {
    $weekGames = [];
    foreach (nsr_series($pdo, $lid, $week) as $s) {
        $h = $teams[$s['home_team_id']] ?? null;
        $a = $teams[$s['away_team_id']] ?? null;
        if (!$h || !$a) continue;
        $weekGames[] = [
            'fixture_id'     => (int)$s['game_id'],
            'home'           => nsr_th_side($h, $records),
            'away'           => nsr_th_side($a, $records),
            'status'         => nsr_th_status($s),
            'forfeit'        => !empty($s['forfeit']),
            'score'          => nsr_th_score($s),
            'series'         => nsr_th_series($s),
            'winner_team_id' => nsr_series_winner($s),
            'when'           => null,
        ];
    }

    $standings = [];
    foreach (nsr_standings_rows($pdo, $lid)['rows'] as $i => $r) {
        $t = $teams[(int)$r['id']] ?? null;
        if (!$t) continue;
        $standings[] = [
            'rank'     => $i + 1,
            'team_id'  => (int)$t['id'],
            'name'     => (string)$t['name'],
            'abbr'     => (string)($t['abbrev'] ?? ''),
            'logo_url' => nsr_th_url($t['logo_path'] ?? null),
            'record'   => (int)$r['w'] . '-' . (int)$r['l'],
        ];
    }

    $done = array_values(array_filter(nsr_series($pdo, $lid), function ($s) use ($teamId) {
        return $s['complete']
            && ((int)$s['home_team_id'] === $teamId || (int)$s['away_team_id'] === $teamId);
    }));
    usort($done, function ($x, $y) {
        return [$y['week_num'], $y['game_id']] <=> [$x['week_num'], $x['game_id']];
    });
    $recent = [];
    foreach (array_slice($done, 0, 5) as $s) {
        $isHome = (int)$s['home_team_id'] === $teamId;
        $oppId  = $isHome ? (int)$s['away_team_id'] : (int)$s['home_team_id'];
        $won    = nsr_series_winner($s);
        $played = (int)$s['played'] > 0;
        $recent[] = [
            'fixture_id'    => (int)$s['game_id'],
            'week'          => (int)$s['week_num'],
            'opponent_abbr' => (string)($teams[$oppId]['abbrev'] ?? ''),
            'home'          => $isHome,
            'result'        => $won === null ? null : ($won === $teamId ? 'W' : 'L'),
            'score'         => $played
                ? ($isHome ? $s['home_points'] . '-' . $s['away_points'] : $s['away_points'] . '-' . $s['home_points'])
                : null,
            'series'        => $played
                ? ($isHome ? $s['home_wins'] . '-' . $s['away_wins'] : $s['away_wins'] . '-' . $s['home_wins'])
                : null,
            'forfeit'       => !empty($s['forfeit']),
        ];
    }

    $names = nsr_player_names($pdo, $lid);
    $rows  = array_filter(nsr_player_stat_rows($pdo, $lid), function ($r) use ($teamId) {
        return (int)$r['team_id'] === $teamId;
    });
    $leaders = [];
    foreach (['pass' => 'pass_td', 'rush' => 'rush_td', 'rec' => 'rec_td'] as $label => $key) {
        $leaders[$label] = [];
        foreach (nsr_rank_rows($rows, $key, 3) as $e) {
            $p = (int)$e['row']['poid'];
            $leaders[$label][] = ['poid' => $p, 'name' => (string)($names[$p] ?? ('Player #' . $p)),
                                  'value' => (int)$e['row'][$key]];
        }
    }

    $pos = nsr_player_positions($pdo, $lid);
    $num = nsr_th_numbers($pdo, $lid);
    $roster = [];
    foreach ((nsr_rosters($pdo, $lid)[$teamId] ?? []) as $p) {
        $roster[] = ['poid' => $p, 'name' => (string)($names[$p] ?? ('Player #' . $p)),
                     'position' => $pos[$p] ?? null, 'number' => $num[$p] ?? null];
    }
    $order = array_flip(['QB', 'RB', 'WR', 'OL', 'DL', 'LB', 'DB']);
    usort($roster, function ($x, $y) use ($order) {
        return [$order[$x['position'] ?? ''] ?? 9, $x['name']] <=> [$order[$y['position'] ?? ''] ?? 9, $y['name']];
    });

    return [
        'week_games'   => $weekGames,
        'standings'    => $standings,
        'recent'       => $recent,
        'leaders'      => $leaders,
        'leaders_unit' => 'TD',
        'roster'       => $roster,
        'site_url'     => NSR_PUBLIC_ORIGIN . '/league/team.php?league_id=' . $lid . '&team_id=' . $teamId,
    ];
}

/**
 * @param int|null $leagueId  the league the caller pins (the Reborn backend's LEAGUE_ID), or null for the
 *                            newest non-hidden league. A pinned league that does not exist is a caller error,
 *                            never member:false: a misconfigured backend must not hide the tab from members.
 */
function nsr_team_home(PDO $pdo, string $discordUserId, ?int $leagueId = null): array {
    $did = trim($discordUserId);
    if (!preg_match('/^\d{15,22}$/', $did)) {
        throw new InvalidArgumentException('discord_user_id must be a Discord snowflake');
    }

    if ($leagueId !== null) {
        $q = $pdo->prepare('SELECT id FROM leagues WHERE id = ?');
        $q->execute([$leagueId]);
        $lid = (int)($q->fetchColumn() ?: 0);
        if ($lid <= 0) throw new InvalidArgumentException('league_id is not a league');
    } else {
        $lid = nsr_th_current_league($pdo);
    }
    if ($lid <= 0) return ['member' => false];

    /* Several owner rows can share one Discord id (one person, two handles over
       the years). Any of them that holds a club in this league makes the person a
       member; lowest team id wins if somehow more than one does. */
    $st = $pdo->prepare(
        "SELECT ot.team_id
           FROM league_owners o JOIN owner_teams ot ON ot.owner_id = o.id
          WHERE o.discord_user_id = ? AND ot.league_id = ?
          ORDER BY ot.team_id LIMIT 1");
    $st->execute([$did, $lid]);
    $teamId = (int)($st->fetchColumn() ?: 0);

    $teams = nsr_teams($pdo, $lid);
    if ($teamId <= 0 || !isset($teams[$teamId])) return ['member' => false];
    $me = $teams[$teamId];

    $L = $pdo->prepare('SELECT name, season_label FROM leagues WHERE id = ?');
    $L->execute([$lid]);
    $lg = $L->fetch(PDO::FETCH_ASSOC) ?: ['name' => '', 'season_label' => null];
    $season = preg_match('/(\d+)/', (string)($lg['season_label'] ?: $lg['name']), $sm) ? (int)$sm[1] : null;

    $week    = nsr_th_week($pdo, $lid);
    $records = nsr_team_records($pdo, $lid);
    $rankOf  = nsr_team_standing($pdo, $lid);
    $owners  = nsr_th_owners($pdo, $lid);

    $all  = nsr_series($pdo, $lid);
    $mine = nsr_th_pick_matchup($all, $teamId, nsr_current_week($pdo, $lid), $week);

    if ($mine === null) {
        $matchup = ['fixture_id' => null, 'week' => $week, 'home' => null, 'status' => 'bye',
                    'forfeit' => false,
                    'score' => null, 'series' => null, 'winner_team_id' => null, 'when' => null,
                    'opponent' => null];
    } else {
        $isHome = (int)$mine['home_team_id'] === $teamId;
        $oppId  = $isHome ? (int)$mine['away_team_id'] : (int)$mine['home_team_id'];
        $opp    = $teams[$oppId] ?? ['id' => $oppId, 'name' => '', 'abbrev' => ''];
        $matchup = [
            'fixture_id'     => (int)$mine['game_id'],
            'week'           => (int)$mine['week_num'],
            'home'           => $isHome,
            'status'         => nsr_th_status($mine),
            'forfeit'        => !empty($mine['forfeit']),
            'score'          => nsr_th_score($mine),
            'series'         => nsr_th_series($mine),
            'winner_team_id' => nsr_series_winner($mine),
            'when'           => null,
            'opponent'       => nsr_th_side($opp, $records) + [
                'owner_display'   => $owners[$oppId]['display'] ?? '',
                'discord_user_id' => $owners[$oppId]['discord'] ?? null,
            ],
        ];
    }

    return [
        'member' => true,
        // false = the league has no fixtures at all yet, so a bye means "no
        // schedule", not "this club sits the week out".
        'schedule_published' => count($all) > 0,
        'league' => [
            'id'         => $lid,
            'name'       => (string)$lg['name'],
            'season'     => $season,
            'week'       => $week,
            'week_label' => nsr_week_label($lid, $week),
        ],
        'team' => [
            'id'       => $teamId,
            'name'     => (string)$me['name'],
            'abbr'     => (string)($me['abbrev'] ?? ''),
            'logo_url' => nsr_th_url($me['logo_path'] ?? null),
            'color'    => (string)($me['primary_color'] ?? ''),
            'record'   => $records[$teamId] ?? '0-0',
            'standing' => $rankOf[$teamId] ?? null,
        ],
        'matchup' => $matchup,
    ] + nsr_th_page($pdo, $lid, $teamId, $teams, $records, $week);
}
