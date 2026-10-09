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

    /* ---- the week's scoreboard ---- */
    $wg = $a['week_games'] ?? [];
    check('one fixture in week 2', count($wg) === 1);
    check('scoreboard fixture id', ($wg[0]['fixture_id'] ?? null) === $g['wk2ba']);
    check('scoreboard home is B', ($wg[0]['home']['team_id'] ?? null) === $t['B']);
    check('scoreboard status', ($wg[0]['status'] ?? null) === 'scheduled');
    check('scoreboard forfeit flag', ($wg[0]['forfeit'] ?? null) === false);
    check('club with no logo has a null logo_url', array_key_exists('logo_url', $wg[0]['home']) && $wg[0]['home']['logo_url'] === null);
    check('non-ASCII club name survives', ($wg[0]['home']['name'] ?? '') === 'Señor B');

    /* ---- standings ---- */
    $sd = $a['standings'] ?? [];
    check('four clubs in the table', count($sd) === 4);
    check('ranks run 1..4', array_column($sd, 'rank') === [1, 2, 3, 4]);
    $rowA = array_values(array_filter($sd, function ($r) use ($t) { return $r['team_id'] === $t['A']; }))[0] ?? [];
    check('A in table at 1-0', ($rowA['record'] ?? null) === '1-0');

    /* ---- recent results ---- */
    $rc = $a['recent'] ?? [];
    check('A has one result', count($rc) === 1);
    check('recent fixture', ($rc[0]['fixture_id'] ?? null) === $g['wk1ab']);
    check('recent opponent', ($rc[0]['opponent_abbr'] ?? null) === 'THB');
    check('recent home flag', ($rc[0]['home'] ?? null) === true);
    check('recent W', ($rc[0]['result'] ?? null) === 'W');
    check('recent points from A side', ($rc[0]['score'] ?? null) === '49-21');
    check('recent series from A side', ($rc[0]['series'] ?? null) === '2-0');

    $d = nsr_team_home($pdo, $did['D']);
    $rd = $d['recent'][0] ?? [];
    check('winner-only series: D result W', ($rd['result'] ?? null) === 'W');
    check('winner-only series: no score', array_key_exists('score', $rd) && $rd['score'] === null);

    /* ---- leaders: A's own players, touchdowns ---- */
    $ld = $a['leaders'] ?? [];
    check('leaders unit', ($a['leaders_unit'] ?? null) === 'TD');
    check('pass leader is poid1 with 3', ($ld['pass'][0]['poid'] ?? null) === $fx['poid'][1] && ($ld['pass'][0]['value'] ?? null) === 3);
    check('pass leader has a name', ($ld['pass'][0]['name'] ?? '') !== '');
    check('rush leader value 1', ($ld['rush'][0]['value'] ?? null) === 1);
    check('B receiver not in A leaders', ($ld['rec'] ?? null) === []);

    /* ---- roster ---- */
    $ro = $a['roster'] ?? [];
    check('three rostered', count($ro) === 3);
    check('roster names', count(array_filter($ro, function ($p) { return $p['name'] !== ''; })) === 3);
    check('roster numbers are int or null', count(array_filter($ro, function ($p) {
        return $p['number'] === null || is_int($p['number']); })) === 3);
    check('roster has position key', array_key_exists('position', $ro[0] ?? []));

    check('site url', ($a['site_url'] ?? '') === "https://nflstreetreborn.com/league/team.php?league_id=$lid&team_id={$t['A']}");

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
    $wg2 = $a2['week_games'][0] ?? [];
    check('scoreboard entry in_progress', ($wg2['status'] ?? null) === 'in_progress');
    check('scoreboard entry score 14-10', ($wg2['score'] ?? null) === ['home' => 14, 'away' => 10]);
    check('scoreboard entry series 1-0', ($wg2['series'] ?? null) === ['home' => 1, 'away' => 0]);
    // Site semantics (nsr_series_winner): mid-series this is the series LEADER, not a settled winner.
    check('scoreboard entry winner_team_id = series leader mid-series', ($wg2['winner_team_id'] ?? null) === $t['B']);

    /* ---- additive fields: forfeit, schedule_published ---- */
    check('schedule_published true', ($a2['schedule_published'] ?? null) === true);
    check('matchup forfeit false', ($a2['matchup']['forfeit'] ?? null) === false);
    check('bye forfeit false', ($c['matchup']['forfeit'] ?? null) === false);
    check('recent forfeit false', ($a2['recent'][0]['forfeit'] ?? null) === false);

    check('payload encodes as JSON', json_encode($a2, JSON_THROW_ON_ERROR) !== '');

    /* ---- postponed fixture: week 1 C-D never played ----
       Clear the C-D result so week 1 has one open fixture while A-B is final.
       The league's week stays on 1, but A's card must move on to its own next
       open series (week 2 B-A), and C's card is the open week-1 C-D. */
    $pdo->prepare("DELETE FROM game_series_scores WHERE league_id = ? AND game_id = ?")->execute([$lid, $g['wk1cd']]);
    $a3 = nsr_team_home($pdo, $did['A']);
    check('postponed: league week held on 1', ($a3['league']['week'] ?? null) === 1);
    check('postponed: A matchup is week-2 B-A', ($a3['matchup']['fixture_id'] ?? null) === $g['wk2ba']);
    check('postponed: A matchup week 2', ($a3['matchup']['week'] ?? null) === 2);
    check('postponed: A matchup still in_progress', ($a3['matchup']['status'] ?? null) === 'in_progress');
    $c3 = nsr_team_home($pdo, $did['C']);
    check('postponed: C matchup is open week-1 C-D', ($c3['matchup']['fixture_id'] ?? null) === $g['wk1cd']);
    check('postponed: C matchup week 1', ($c3['matchup']['week'] ?? null) === 1);
    check('postponed: C matchup scheduled', ($c3['matchup']['status'] ?? null) === 'scheduled');
    check('postponed: C opponent is D', ($c3['matchup']['opponent']['team_id'] ?? null) === $t['D']);
    check('postponed: week games = week 1', array_column($a3['week_games'] ?? [], 'fixture_id') === [$g['wk1ab'], $g['wk1cd']]);

    /* ---- recent: capped at 5, newest first; a forfeit flagged ----
       Six more settled A-C series in weeks 3..8 (winner row only); week 8 is a
       forfeit. A then has seven results; recent keeps weeks 8..4. */
    $insG = $pdo->prepare("INSERT INTO games (league_id, week_num, week_no, series_game_no, home_team_id, away_team_id, status)
                           VALUES (?,?,?,1,?,?,?)");
    $insW = $pdo->prepare("INSERT INTO game_series_scores (league_id, game_id, series_game_no, winner_team_id, status)
                           VALUES (?,?,0,?,'complete')");
    for ($w = 3; $w <= 8; $w++) {
        $insG->execute([$lid, $w, $w, $t['A'], $t['C'], $w === 8 ? 'forfeit' : 'played']);
        $insW->execute([$lid, (int)$pdo->lastInsertId(), $t['A']]);
    }
    $a4 = nsr_team_home($pdo, $did['A']);
    $rc4 = $a4['recent'] ?? [];
    check('recent capped at 5', count($rc4) === 5);
    check('recent newest first (weeks 8..4)', array_column($rc4, 'week') === [8, 7, 6, 5, 4]);
    check('recent forfeit flagged on week 8', ($rc4[0]['forfeit'] ?? null) === true);
    check('recent non-forfeit not flagged', ($rc4[1]['forfeit'] ?? null) === false);
    check('recent forfeit still a W', ($rc4[0]['result'] ?? null) === 'W');

    /* ---- no schedule at all: a newer league with a club but no fixtures ---- */
    $pdo->prepare("INSERT INTO leagues (name, season_label, pool_version_id, weeks_regular_season)
                   SELECT 'TH Empty League 7', '', pool_version_id, 2 FROM leagues WHERE id = ?")->execute([$lid]);
    $fx['lid2'] = (int)$pdo->lastInsertId();
    $pdo->prepare("INSERT INTO teams (league_id, name, abbrev) VALUES (?, 'TH Empty Club', 'THE')")->execute([$fx['lid2']]);
    $emptyTeam = (int)$pdo->lastInsertId();
    $pdo->prepare("INSERT INTO league_owners (handle, display_name, discord_user_id) VALUES ('THOWNERZ', NULL, '900000000000000077')")->execute();
    $pdo->prepare("INSERT INTO owner_teams (owner_id, league_id, team_id) VALUES (?,?,?)")
        ->execute([(int)$pdo->lastInsertId(), $fx['lid2'], $emptyTeam]);
    $z = nsr_team_home($pdo, '900000000000000077');
    check('empty league: member', ($z['member'] ?? null) === true && ($z['league']['id'] ?? null) === $fx['lid2']);
    check('empty league: schedule_published false', ($z['schedule_published'] ?? null) === false);
    check('empty league: matchup is a bye', ($z['matchup']['status'] ?? null) === 'bye' && ($z['matchup']['forfeit'] ?? null) === false);
    check('empty league: no week games', ($z['week_games'] ?? null) === []);
    check('empty season_label falls back to the league name', ($z['league']['season'] ?? null) === 7);

    /* ---- explicit league_id: the backend pins the league (LEAGUE_ID) ---- */
    // lid2 (the empty league) is now the newest non-hidden one, so the fallback picks it...
    check('fallback picks the newest league', nsr_th_current_league($pdo) === $fx['lid2']);
    check('fallback: A is not a member of the newest league', nsr_team_home($pdo, $did['A']) === ['member' => false]);
    // ...but an explicit league_id pins the fixture league whatever is newer.
    $p = nsr_team_home($pdo, $did['A'], $lid);
    check('pinned: A is a member', ($p['member'] ?? null) === true);
    check('pinned: league is the fixture league', ($p['league']['id'] ?? null) === $lid);
    check('pinned: A team', ($p['team']['id'] ?? null) === $t['A']);
    $threw = '';
    try { nsr_team_home($pdo, $did['A'], 999999999); } catch (InvalidArgumentException $e) { $threw = $e->getMessage(); }
    check('unknown league_id is refused, not member:false', $threw === 'league_id is not a league');
} finally {
    th_cleanup($pdo, $fx);
}
echo $fails ? "\n$fails FAILED\n" : "\nALL PASS\n";
exit($fails ? 1 : 0);
