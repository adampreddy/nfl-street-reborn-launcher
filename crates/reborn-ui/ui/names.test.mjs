// node --test crates/reborn-ui/ui/names.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shownName, discordName, nameWithDiscord } from './names.js';

test('a nickname wins, the Discord name stays beside it', () => {
  const p = { display_name: 'adam_discord', nickname: 'Slick' };
  assert.equal(shownName(p), 'Slick');
  assert.equal(discordName(p), 'adam_discord');
  assert.equal(nameWithDiscord(p), 'Slick (@adam_discord)');
});

test('no nickname (unset, an older server, blank): the Discord name, nothing beside it', () => {
  for (const p of [{ display_name: 'Adam' }, { display_name: 'Adam', nickname: null }, { display_name: 'Adam', nickname: '  ' }]) {
    assert.equal(shownName(p), 'Adam');
    assert.equal(discordName(p), null);
    assert.equal(nameWithDiscord(p), 'Adam');
  }
  assert.equal(discordName({ display_name: 'Adam', nickname: 'adam' }), null, 'the same name in other case adds nothing');
  assert.equal(shownName(null), '');
  assert.equal(nameWithDiscord({}, 'Opponent'), 'Opponent');
});

test("the server's own `name` is not trusted over the raw fields", () => {
  assert.equal(shownName({ name: 'Falcons', display_name: 'Adam' }), 'Adam');
});
