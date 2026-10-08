<?php
declare(strict_types=1);

/**
 * A throwaway league for the team_home tests. Call inside a transaction and
 * roll back afterwards; nothing here is meant to survive.
 *
 *   week 1  A (home) beat B 2-0 (21-14, 28-7)
 *           C vs D decided by series row only: D won, no game scores
 *   week 2  B (home) vs A, nothing played yet
 *           C and D have a bye
 *   owners  A..D own the four clubs; E owns a club only in an OLDER league;
 *           F is a second owner row sharing A's Discord id with no club.
 */
function th_fixture(PDO $pdo): array {
    $pool = (int)$pdo->query(
        "SELECT version_id FROM pool_csv_rows GROUP BY version_id ORDER BY version_id DESC LIMIT 1")->fetchColumn();
    $older = (int)$pdo->query("SELECT MAX(id) FROM leagues")->fetchColumn();

    $pdo->prepare("INSERT INTO leagues (name, season_label, pool_version_id, weeks_regular_season)
                   VALUES ('TH Test League', 'Season 9', ?, 2)")->execute([$pool]);
    $lid = (int)$pdo->lastInsertId();

    $t = [];
    $teams = ['A' => ['Team A', '#112233', "/league/uploads/league_$lid/teams/a.png"],
              'B' => ['Señor B', '#445566', null],
              'C' => ['Team C', '#778899', null],
              'D' => ['Team D', '#AABBCC', null]];
    $insT = $pdo->prepare("INSERT INTO teams (league_id, name, abbrev, primary_color, logo_path) VALUES (?,?,?,?,?)");
    foreach ($teams as $k => [$name, $color, $logo]) {
        $insT->execute([$lid, $name, "TH$k", $color, $logo]);
        $t[$k] = (int)$pdo->lastInsertId();
    }
    $pdo->prepare("INSERT INTO teams (league_id, name, abbrev) VALUES (?, 'TH Old Club', 'THOLD')")->execute([$older]);
    $oldTeam = (int)$pdo->lastInsertId();

    $did = ['A' => '900000000000000001', 'B' => '900000000000000002', 'C' => '900000000000000003',
            'D' => '900000000000000004', 'E' => '900000000000000005', 'F' => '900000000000000001'];
    $o = [];
    $insO = $pdo->prepare("INSERT INTO league_owners (handle, display_name, discord_user_id) VALUES (?,?,?)");
    foreach ($did as $k => $d) {
        $insO->execute(["THOWNER$k", $k === 'B' ? 'Bee' : null, $d]);
        $o[$k] = (int)$pdo->lastInsertId();
    }
    $insOT = $pdo->prepare("INSERT INTO owner_teams (owner_id, league_id, team_id) VALUES (?,?,?)");
    foreach (['A', 'B', 'C', 'D'] as $k) $insOT->execute([$o[$k], $lid, $t[$k]]);
    $insOT->execute([$o['E'], $older, $oldTeam]);

    $insG = $pdo->prepare("INSERT INTO games (league_id, week_num, week_no, series_game_no, home_team_id, away_team_id)
                           VALUES (?,?,?,1,?,?)");
    $g = [];
    $insG->execute([$lid, 1, 1, $t['A'], $t['B']]); $g['wk1ab'] = (int)$pdo->lastInsertId();
    $insG->execute([$lid, 1, 1, $t['C'], $t['D']]); $g['wk1cd'] = (int)$pdo->lastInsertId();
    $insG->execute([$lid, 2, 2, $t['B'], $t['A']]); $g['wk2ba'] = (int)$pdo->lastInsertId();

    $insS = $pdo->prepare("INSERT INTO game_series_scores
                             (league_id, game_id, series_game_no, home_score, away_score, winner_team_id, status)
                           VALUES (?,?,?,?,?,?,'complete')");
    $insS->execute([$lid, $g['wk1ab'], 1, 21, 14, null]);
    $insS->execute([$lid, $g['wk1ab'], 2, 28, 7, null]);
    $insS->execute([$lid, $g['wk1cd'], 0, null, null, $t['D']]);   // series row only

    $poids = array_map('intval', $pdo->query(
        "SELECT poid FROM pool_csv_rows WHERE version_id = $pool ORDER BY poid LIMIT 4")->fetchAll(PDO::FETCH_COLUMN));
    $insR = $pdo->prepare("INSERT INTO league_player_rights (league_id, poid, team_id, status) VALUES (?,?,?,'rostered')");
    foreach ([0, 1, 2] as $i) $insR->execute([$lid, $poids[$i], $t['A']]);
    $insR->execute([$lid, $poids[3], $t['B']]);

    $insE = $pdo->prepare("INSERT INTO stat_events (league_id, game_id, series_game_no, team_id, player_poid, stat_key, delta)
                           VALUES (?,?,?,?,?,?,?)");
    $insE->execute([$lid, $g['wk1ab'], 1, $t['A'], $poids[0], 'pass_td', 2]);
    $insE->execute([$lid, $g['wk1ab'], 2, $t['A'], $poids[0], 'pass_td', 1]);
    $insE->execute([$lid, $g['wk1ab'], 1, $t['A'], $poids[1], 'rush_td', 1]);
    $insE->execute([$lid, $g['wk1ab'], 1, $t['B'], $poids[3], 'rec_td', 5]);

    return ['lid' => $lid, 'old_team' => $oldTeam, 't' => $t, 'did' => $did, 'g' => $g,
            'poid' => [1 => $poids[0], 2 => $poids[1], 3 => $poids[2], 4 => $poids[3]]];
}

/**
 * Roll back the fixture. nsr_week_label() -> get_playoff_config() runs
 * CREATE/ALTER TABLE on every call, and DDL commits the open transaction in
 * MySQL/MariaDB, so rollBack() alone can leave the fixture behind. When the
 * transaction is gone, delete the fixture rows by id instead.
 */
function th_cleanup(PDO $pdo, ?array $fx): void {
    if ($pdo->inTransaction()) { $pdo->rollBack(); return; }
    if ($fx === null) return;
    $lid = (int)$fx['lid'];
    $old = (int)($fx['old_team'] ?? 0);
    foreach (['stat_events', 'league_player_rights', 'game_series_scores', 'owner_teams', 'games', 'teams'] as $tb) {
        $pdo->exec("DELETE FROM $tb WHERE league_id = $lid");
    }
    if ($old > 0) {
        $pdo->exec("DELETE FROM owner_teams WHERE team_id = $old");
        $pdo->exec("DELETE FROM teams WHERE id = $old");
    }
    $pdo->exec("DELETE FROM league_owners WHERE handle LIKE 'THOWNER%'");
    $pdo->exec("DELETE FROM league_playoff_config WHERE league_id = $lid");
    $pdo->exec("DELETE FROM leagues WHERE id = $lid");
}
