// Strict client isolation: all wireless paths blocked. The cascade
// must arrive at the USB prompt without prompting the user to choose
// a method.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnHarnessPair, expectEvent, simulator } from "./_harness.ts";

test("isolated network → USB prompt engages automatically", async (t) => {
    await simulator("block-mdns.sh", "on");
    await simulator("block-broadcast.sh", "on");
    await simulator("isolate-subnet.sh", "on");
    const harness = await spawnHarnessPair({ profile: "happy" });

    t.after(async () => {
        await harness.shutdown();
        await simulator("isolate-subnet.sh", "off");
        await simulator("block-broadcast.sh", "off");
        await simulator("block-mdns.sh", "off");
    });

    const status = await expectEvent(harness.pc, "status_key", 14_000, (e) =>
        e.key === "cascade.usb.prompt",
    );
    assert.equal(status.key, "cascade.usb.prompt");
    // Critically: the user was never asked to pick a method.
    assert.ok(
        !harness.pc.events.some((e: any) => e.kind === "method_picker"),
        "method picker event must never fire",
    );
});
