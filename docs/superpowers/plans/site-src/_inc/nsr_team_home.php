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

/** team_id => owner display name and Discord id, for one league. */
function nsr_th_owners(PDO $pdo, int $leagueId): array {
    $st = $pdo->prepare(
        "SELECT ot.team_id, o.handle, o.display_name, o.discord_user_id
           FROM owner_teams ot JOIN league_owners o ON o.id = ot.owner_id
          WHERE ot.league_id = ?");
    $st->execute([$leagueId]);
    $out = [];
    foreach ($st->fetchAll(PDO::FETCH_ASSOC) as $r) {
        $d = trim((string)($r['discord_user_id'] ?? ''));
        $out[(int)$r['team_id']] = [
            'display' => (string)($r['display_name'] ?: $r['handle']),
            'discord' => $d !== '' ? $d : null,
        ];
    }
    return $out;
}

function nsr_team_home(PDO $pdo, string $discordUserId): array {
    $did = trim($discordUserId);
    if (!preg_match('/^\d{15,22}$/', $did)) {
        throw new InvalidArgumentException('discord_user_id must be a Discord snowflake');
    }

    $lid = nsr_th_current_league($pdo);
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
    $season = preg_match('/(\d+)/', (string)($lg['season_label'] ?? $lg['name']), $sm) ? (int)$sm[1] : null;

    $week    = nsr_th_week($pdo, $lid);
    $records = nsr_team_records($pdo, $lid);
    $rankOf  = nsr_team_standing($pdo, $lid);
    $owners  = nsr_th_owners($pdo, $lid);

    $mine = null;
    foreach (nsr_series($pdo, $lid, $week) as $s) {
        if ((int)$s['home_team_id'] === $teamId || (int)$s['away_team_id'] === $teamId) { $mine = $s; break; }
    }

    if ($mine === null) {
        $matchup = ['fixture_id' => null, 'week' => $week, 'home' => null, 'status' => 'bye',
                    'score' => null, 'series' => null, 'winner_team_id' => null, 'when' => null,
                    'opponent' => null];
    } else {
        $isHome = (int)$mine['home_team_id'] === $teamId;
        $oppId  = $isHome ? (int)$mine['away_team_id'] : (int)$mine['home_team_id'];
        $opp    = $teams[$oppId] ?? ['id' => $oppId, 'name' => '', 'abbrev' => ''];
        $matchup = [
            'fixture_id'     => (int)$mine['game_id'],
            'week'           => $week,
            'home'           => $isHome,
            'status'         => nsr_th_status($mine),
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
    ];
}
