// Parity check: the 256 emojis must be identical in both apps and in
// the canonical EMOJI_SET.md document. A mismatch would silently
// produce different verifiers on each side of a successful pair.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");

function emojisFromRust(): string[] {
    const src = readFileSync(
        resolve(root, "desktop/src-tauri/src/pairing/emoji_code.rs"),
        "utf8",
    );
    const start = src.indexOf("EMOJIS: [&str; 256] = [");
    const end = src.indexOf("];", start);
    const block = src.slice(start, end);
    return Array.from(block.matchAll(/"([^"]+)"/g)).map((m) => m[1]);
}

function emojisFromKotlin(): string[] {
    const src = readFileSync(
        resolve(root, "android/app/src/main/kotlin/io/tether/pairing/EmojiSet.kt"),
        "utf8",
    );
    const start = src.indexOf("listOf(");
    const end = src.indexOf(")\n", start);
    const block = src.slice(start, end);
    return Array.from(block.matchAll(/"([^"]+)"/g)).map((m) => m[1]);
}

function emojisFromMarkdown(): string[] {
    const src = readFileSync(resolve(root, "protocol/EMOJI_SET.md"), "utf8");
    const rows = src.matchAll(/\|\s*(\d+)\s*\|\s*([^\s|]+)\s*\|/g);
    return Array.from(rows).map((m) => m[2]);
}

test("rust ↔ kotlin ↔ markdown emoji tables match", () => {
    const rust = emojisFromRust();
    const kotlin = emojisFromKotlin();
    const md = emojisFromMarkdown();

    assert.equal(rust.length, 256, `Rust table has ${rust.length} entries`);
    assert.equal(kotlin.length, 256, `Kotlin table has ${kotlin.length} entries`);
    assert.equal(md.length, 256, `Markdown table has ${md.length} entries`);
    for (let i = 0; i < 256; i++) {
        assert.equal(
            rust[i],
            kotlin[i],
            `divergence at index ${i}: rust="${rust[i]}" kotlin="${kotlin[i]}"`,
        );
        assert.equal(
            rust[i],
            md[i],
            `divergence at index ${i}: rust="${rust[i]}" markdown="${md[i]}"`,
        );
    }
});
