<?php
declare(strict_types=1);

/* php C:\Users\Adam\nsr_local\tests\team_home_test.php
   Builds a throwaway league in a transaction, checks nsr_team_home() against it,
   rolls back. Exit 0 = every check passed. */

require_once __DIR__ . '/../public_html/league/_inc/config.php';
require_once __DIR__ . '/../public_html/league/_inc/db.php';
require_once __DIR__ . '/../public_html/league/_inc/nsr_team_home.php';
require_once __DIR__ . '/th_fixture.php';

$fails = 0;
function check(string $what, bool $ok): void {
    global $fails;
    echo ($ok ? 'PASS ' : 'FAIL ') . $what . "\n";
    if (!$ok) $fails++;
}

$pdo = db();
$pdo->beginTransaction();
$fx = null;
try {
    $fx = th_fixture($pdo);
    $t = $fx['t']; $did = $fx['did']; $g = $fx['g']; $lid = $fx['lid'];

    /* ---- membership ---- */
    $threw = false;
    try { nsr_team_home($pdo, 'abc'); } catch (InvalidArgumentException $e) { $threw = true; }
    check('non-snowflake id is refused', $threw);
    check('unknown id is not a member', nsr_team_home($pdo, '900000000000000099') === ['member' => false]);
    check('owner of an older season only is not a member', nsr_team_home($pdo, $did['E']) === ['member' => false]);
    check('current league is the fixture league', nsr_th_current_league($pdo) === $lid);

    /* ---- A: league, team, matchup ---- */
    $a = nsr_team_home($pdo, $did['A']);   // F shares this id and owns nothing: A must still resolve
    check('A is a member', ($a['member'] ?? null) === true);
    check('league id', ($a['league']['id'] ?? null) === $lid);
    check('league season parsed from season_label', ($a['league']['season'] ?? null) === 9);
    check('finished week 1 advances the tab to week 2', ($a['league']['week'] ?? null) === 2);
    check('week label', ($a['league']['week_label'] ?? '') !== '');
    check('team id', ($a['team']['id'] ?? null) === $t['A']);
    check('team abbr', ($a['team']['abbr'] ?? null) === 'THA');
    check('team logo absolute',
          ($a['team']['logo_url'] ?? null) === "https://nflstreetreborn.com/league/uploads/league_$lid/teams/a.png");
    check('team color', ($a['team']['color'] ?? null) === '#112233');
    check('team record', ($a['team']['record'] ?? null) === '1-0');
    check('team standing is 1 or 2', in_array($a['team']['standing'] ?? 0, [1, 2], true));

    $m = $a['matchup'] ?? [];
    check('matchup fixture is week 2 B-A', ($m['fixture_id'] ?? null) === $g['wk2ba']);
    check('matchup week', ($m['week'] ?? null) === 2);
    check('A is away', ($m['home'] ?? null) === false);
    check('matchup scheduled', ($m['status'] ?? null) === 'scheduled');
    check('scheduled has no score', array_key_exists('score', $m) && $m['score'] === null);
    check('when is null', array_key_exists('when', $m) && $m['when'] === null);
    check('opponent is B', ($m['opponent']['team_id'] ?? null) === $t['B']);
    check('opponent owner display', ($m['opponent']['owner_display'] ?? null) === 'Bee');
    check('opponent discord id is a string', ($m['opponent']['discord_user_id'] ?? null) === $did['B']);
    check('opponent record', ($m['opponent']['record'] ?? null) === '0-1');

    /* ---- C: bye ---- */
    $c = nsr_team_home($pdo, $did['C']);
    check('C has a bye', ($c['matchup']['status'] ?? null) === 'bye');
    check('bye has no opponent', array_key_exists('opponent', $c['matchup']) && $c['matchup']['opponent'] === null);
    check('bye has no fixture', $c['matchup']['fixture_id'] === null);

    // --- payload checks (Task 3) ---

    /* ---- game 1 of week 2 played: B won 14-10, game 2 still to come ----
       The game-2 row matters: nsr_series() treats a fixture whose ONLY row is one
       complete game as settled (single-game leagues), so a best-of-3 in progress
       needs its next game's row present, as the site's ingest writes it. */
    $pdo->prepare("INSERT INTO game_series_scores (league_id, game_id, series_game_no, home_score, away_score, status)
                   VALUES (?,?,1,14,10,'complete'), (?,?,2,NULL,NULL,'scheduled')")
        ->execute([$lid, $g['wk2ba'], $lid, $g['wk2ba']]);
    $a2 = nsr_team_home($pdo, $did['A']);
    check('one game in: in_progress', ($a2['matchup']['status'] ?? null) === 'in_progress');
    check('series 1-0 to home', ($a2['matchup']['series'] ?? null) === ['home' => 1, 'away' => 0]);
    check('score 14-10', ($a2['matchup']['score'] ?? null) === ['home' => 14, 'away' => 10]);
    check('still week 2', ($a2['league']['week'] ?? null) === 2);

    check('payload encodes as JSON', json_encode($a2, JSON_THROW_ON_ERROR) !== '');
} finally {
    th_cleanup($pdo, $fx);
}
echo $fails ? "\n$fails FAILED\n" : "\nALL PASS\n";
exit($fails ? 1 : 0);
