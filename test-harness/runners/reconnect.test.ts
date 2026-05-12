// After a successful pair, restarting both apps must reconnect with
// zero user action.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnHarnessPair, expectEvent } from "./_harness.ts";

test("restart after pair → silent reconnect", async (t) => {
    const first = await spawnHarnessPair({ profile: "happy" });
    const pcCard = await expectEvent(first.pc, "card", 8_000);
    const phoneCard = await expectEvent(first.phone, "card", 8_000);
    assert.deepEqual(pcCard.emojis, phoneCard.emojis);
    first.pc.send({ kind: "user_confirm" });
    first.phone.send({ kind: "user_confirm" });
    await expectEvent(first.pc, "paired", 6_000);
    await expectEvent(first.phone, "paired", 6_000);
    await first.shutdown();

    const second = await spawnHarnessPair({ profile: "happy", preserveStore: true });
    t.after(() => second.shutdown());

    const paired = await expectEvent(second.pc, "paired", 8_000);
    assert.equal(typeof paired.peer_device_name, "string");

    // The pairing card MUST NOT be shown on reconnect.
    const card = second.pc.events.find((e: any) => e.kind === "card");
    assert.equal(card, undefined, "card event fired on silent reconnect");
});
