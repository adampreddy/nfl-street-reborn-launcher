<?php
declare(strict_types=1);

/* Start the site first (separate PowerShell):
     php -S 127.0.0.1:8765 -t C:\Users\Adam\nsr_local\public_html
   then: php C:\Users\Adam\nsr_local\tests\team_home_http_test.php
   Uses the LOCAL replica token (_inc/ingest_token.php), never the live one. */

require_once __DIR__ . '/../public_html/league/_inc/ingest_token.php';

$url = getenv('TH_URL') ?: 'http://127.0.0.1:8765/league/api/bot.php';
$fails = 0;
function check(string $what, bool $ok): void {
    global $fails;
    echo ($ok ? 'PASS ' : 'FAIL ') . $what . "\n";
    if (!$ok) $fails++;
}
/** [status, decoded body] */
function call(string $url, string $method, ?string $token, $body): array {
    $h = "Content-Type: application/json\r\n";
    if ($token !== null) $h .= "Authorization: Bearer $token\r\n";
    $ctx = stream_context_create(['http' => ['method' => $method, 'header' => $h,
        'content' => $body === null ? '' : json_encode($body), 'ignore_errors' => true]]);
    $raw = file_get_contents($url, false, $ctx);
    preg_match('~^HTTP/\S+\s+(\d+)~', $http_response_header[0] ?? '', $m);
    return [(int)($m[1] ?? 0), json_decode((string)$raw, true)];
}

[$s, $b] = call($url, 'POST', null, ['action' => 'team_home', 'discord_user_id' => '900000000000000099']);
check('no token -> 401', $s === 401);

[$s, $b] = call($url, 'GET', INGEST_TOKEN, null);
check('GET -> 405', $s === 405);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => 'abc']);
check('bad id -> 400', $s === 400 && ($b['ok'] ?? null) === false);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home']);
check('missing id -> 400', $s === 400);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => '900000000000000099']);
check('unknown id -> 200 member:false', $s === 200 && $b === ['ok' => true, 'member' => false]);

/* A real local owner (ADAM). Locally his newest club may not be in the newest
   league, so only the shape is checked. */
[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => '1132855005523746906']);
check('real owner -> 200 ok with member flag', $s === 200 && ($b['ok'] ?? null) === true && is_bool($b['member'] ?? null));
if (!empty($b['member'])) {
    foreach (['league', 'team', 'matchup', 'week_games', 'standings', 'recent', 'leaders', 'leaders_unit', 'roster', 'site_url'] as $k)
        check("member payload has $k", array_key_exists($k, $b));
}

echo $fails ? "\n$fails FAILED\n" : "\nALL PASS\n";
exit($fails ? 1 : 0);
