// Shared harness scaffolding for the cascade integration tests.
//
// Spawns one PC binary + one Android emulator instance, both pointed
// at private discovery ports so concurrent test runs don't trip over
// each other. Exposes a typed event stream per process.
//
// This is the spec; the real test runner wires it up to the actual
// binaries — keeping it as a stub in the public scaffold lets the
// other test files compile and import a single source of truth.

import { spawn, type ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export type HarnessOptions = {
    profile: "happy" | "isolated";
    preserveStore?: boolean;
};

export interface HarnessProcess extends EventEmitter {
    send(msg: unknown): void;
    events: any[];
}

export type HarnessPair = {
    pc: HarnessProcess;
    phone: HarnessProcess;
    shutdown(): Promise<void>;
};

export async function spawnHarnessPair(_opts: HarnessOptions): Promise<HarnessPair> {
    // The full implementation spawns both binaries with a JSONL event
    // protocol on stdout. Here we expose the contract; CI runs against
    // the real binaries once they're built.
    const pcDataDir = mkdtempSync(join(tmpdir(), "tether-pc-"));
    const phoneDataDir = mkdtempSync(join(tmpdir(), "tether-phone-"));

    const pc = makeStubProcess();
    const phone = makeStubProcess();

    return {
        pc,
        phone,
        async shutdown() {
            try { rmSync(pcDataDir, { recursive: true, force: true }); } catch {}
            try { rmSync(phoneDataDir, { recursive: true, force: true }); } catch {}
        },
    };
}

function makeStubProcess(): HarnessProcess {
    const e = new EventEmitter() as HarnessProcess;
    e.events = [];
    e.send = (_msg: unknown) => {};
    return e;
}

export async function expectEvent(
    proc: HarnessProcess,
    kind: string,
    timeoutMs: number,
    predicate?: (evt: any) => boolean,
): Promise<any> {
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            reject(new Error(`expected ${kind} within ${timeoutMs}ms`));
        }, timeoutMs);
        const handler = (evt: any) => {
            if (evt.kind !== kind) return;
            if (predicate && !predicate(evt)) return;
            clearTimeout(timer);
            proc.off("event", handler);
            resolve(evt);
        };
        proc.on("event", handler);
    });
}

export async function simulator(script: string, action: "on" | "off"): Promise<void> {
    return new Promise((resolve, reject) => {
        const proc: ChildProcess = spawn(
            "sudo",
            [`${import.meta.dirname}/../network-simulator/${script}`, action],
            { stdio: "inherit" },
        );
        proc.on("exit", (code) => {
            if (code === 0) resolve();
            else reject(new Error(`${script} ${action} exited with ${code}`));
        });
    });
}
