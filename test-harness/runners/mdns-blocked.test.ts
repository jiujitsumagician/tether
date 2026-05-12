// With mDNS dropped, the cascade must fall through to UDP broadcast
// and still complete pairing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnHarnessPair, expectEvent, simulator } from "./_harness.ts";

test("mDNS blocked → UDP broadcast still pairs", async (t) => {
    await simulator("block-mdns.sh", "on");
    const harness = await spawnHarnessPair({ profile: "happy" });

    t.after(async () => {
        await harness.shutdown();
        await simulator("block-mdns.sh", "off");
    });

    // Cascade must visit phase 2 (UDP) before producing a peer.
    const phaseLog: string[] = [];
    harness.pc.on("status_key", (k: string) => phaseLog.push(k));

    const pcCard = await expectEvent(harness.pc, "card", 12_000);
    const phoneCard = await expectEvent(harness.phone, "card", 12_000);
    assert.deepEqual(pcCard.emojis, phoneCard.emojis);

    assert.ok(
        phaseLog.includes("cascade.mdns") && phaseLog.includes("cascade.fallback"),
        `expected cascade.mdns + cascade.fallback in ${JSON.stringify(phaseLog)}`,
    );

    harness.pc.send({ kind: "user_confirm" });
    harness.phone.send({ kind: "user_confirm" });
    await expectEvent(harness.pc, "paired", 6_000);
    await expectEvent(harness.phone, "paired", 6_000);
});
