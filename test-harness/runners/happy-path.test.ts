// Asserts the happy path: both apps run on the same loopback, mDNS
// works, pair completes in under 15 seconds, three matching emojis
// appear on each side.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnHarnessPair, expectEvent } from "./_harness.ts";

test("happy path pairs in under 15 seconds", async (t) => {
    const harness = await spawnHarnessPair({ profile: "happy" });
    t.after(() => harness.shutdown());

    const start = Date.now();
    const pcCard = await expectEvent(harness.pc, "card", 8_000);
    const phoneCard = await expectEvent(harness.phone, "card", 8_000);

    // Same three emojis on both sides.
    assert.deepEqual(pcCard.emojis, phoneCard.emojis);

    // Both sides confirm.
    harness.pc.send({ kind: "user_confirm" });
    harness.phone.send({ kind: "user_confirm" });

    const pcPaired = await expectEvent(harness.pc, "paired", 6_000);
    const phonePaired = await expectEvent(harness.phone, "paired", 6_000);
    const elapsed = Date.now() - start;

    assert.ok(elapsed < 15_000, `pair took ${elapsed}ms (target: 15000)`);
    assert.equal(typeof pcPaired.peer_device_name, "string");
    assert.equal(typeof phonePaired.peer_device_name, "string");
});
