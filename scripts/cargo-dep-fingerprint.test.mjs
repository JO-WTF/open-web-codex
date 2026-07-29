import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  unlinkSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  checkFingerprint,
  computeInputFingerprint,
  parseCargoDepInfo,
  recordFingerprint,
} from "./cargo-dep-fingerprint.mjs";

const buildContext = {
  component: "platform-server",
  profile: "dev-small",
  build_command: "cargo build --locked --profile dev-small -p open-web-codex-server",
  platform: "test",
  architecture: "test",
  cargo: "cargo test",
  rustc: "rustc test",
  environment: {},
};

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "cargo-fingerprint-"));
  const crate = join(root, "crate with space");
  const source = join(crate, "src");
  const target = join(root, "target", "dev-small");
  mkdirSync(source, { recursive: true });
  mkdirSync(target, { recursive: true });
  writeFileSync(join(root, "Cargo.toml"), "[workspace]\nmembers = [\"crate with space\"]\n");
  writeFileSync(join(root, "Cargo.lock"), "version = 4\n");
  writeFileSync(join(crate, "Cargo.toml"), "[package]\nname = \"example\"\nversion = \"0.1.0\"\n");
  const sourcePath = join(source, "main.rs");
  writeFileSync(sourcePath, "fn main() {}\n");
  const artifact = join(target, "open-web-codex-server");
  writeFileSync(artifact, "binary\n", { mode: 0o700 });
  const depInfo = join(target, "open-web-codex-server.d");
  writeFileSync(
    depInfo,
    `${artifact.replaceAll(" ", "\\ ")}: ${sourcePath.replaceAll(" ", "\\ ")}\n`,
  );
  const stamp = join(root, "stamps", "open-web-codex-server.json");
  const options = {
    workspace: root,
    "dep-info": depInfo,
    artifact,
    stamp,
    component: "platform-server",
    profile: "dev-small",
    "build-command": buildContext.build_command,
  };
  return { root, sourcePath, artifact, depInfo, stamp, options };
}

test("parses escaped Cargo dep-info paths", () => {
  const root = "/tmp/workspace";
  assert.deepEqual(
    parseCargoDepInfo(
      "/tmp/workspace/target/codex: /tmp/workspace/crate\\ with\\ space/src/main.rs\n",
      root,
    ),
    ["/tmp/workspace/crate with space/src/main.rs"],
  );
});

test("records and reuses an exact Cargo component fingerprint", () => {
  const value = fixture();
  try {
    assert.equal(checkFingerprint(value.options, buildContext).fresh, false);
    recordFingerprint(value.options, buildContext);
    assert.deepEqual(checkFingerprint(value.options, buildContext), {
      fresh: true,
      reason: "4 exact Cargo inputs matched",
    });
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("detects content changes even when the source mtime is restored", () => {
  const value = fixture();
  try {
    recordFingerprint(value.options, buildContext);
    const before = new Date("2026-01-01T00:00:00Z");
    utimesSync(value.sourcePath, before, before);
    const original = readFileSync(value.sourcePath, "utf8");
    writeFileSync(value.sourcePath, original.replace("{}", "{ println!(\"changed\"); }"));
    utimesSync(value.sourcePath, before, before);
    assert.deepEqual(checkFingerprint(value.options, buildContext), {
      fresh: false,
      reason: "Cargo build inputs changed",
    });
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("ignores files outside Cargo dep-info", () => {
  const value = fixture();
  try {
    recordFingerprint(value.options, buildContext);
    writeFileSync(join(value.root, "README.md"), "documentation only\n");
    assert.equal(checkFingerprint(value.options, buildContext).fresh, true);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("tracks files added beneath a Cargo directory dependency", () => {
  const value = fixture();
  try {
    const assets = join(value.root, "crate with space", "assets");
    mkdirSync(assets);
    writeFileSync(
      value.depInfo,
      `${value.artifact.replaceAll(" ", "\\ ")}: ${value.sourcePath.replaceAll(" ", "\\ ")} ${assets.replaceAll(" ", "\\ ")}\n`,
    );
    recordFingerprint(value.options, buildContext);
    writeFileSync(join(assets, "new-template.md"), "new build input\n");
    assert.deepEqual(checkFingerprint(value.options, buildContext), {
      fresh: false,
      reason: "Cargo build inputs changed",
    });
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("detects manifest, build context, artifact, and dep-info changes", () => {
  const value = fixture();
  try {
    recordFingerprint(value.options, buildContext);
    writeFileSync(
      join(value.root, "crate with space", "Cargo.toml"),
      "[package]\nname = \"example\"\nversion = \"0.2.0\"\n",
    );
    assert.equal(checkFingerprint(value.options, buildContext).fresh, false);

    recordFingerprint(value.options, buildContext);
    assert.equal(
      checkFingerprint(value.options, { ...buildContext, rustc: "rustc changed" }).fresh,
      false,
    );

    assert.equal(
      checkFingerprint(value.options, {
        ...buildContext,
        build_command: "cargo build --locked --release -p open-web-codex-server",
      }).fresh,
      false,
    );

    recordFingerprint(value.options, buildContext);
    writeFileSync(value.artifact, "different binary\n", { mode: 0o700 });
    assert.equal(checkFingerprint(value.options, buildContext).fresh, false);

    recordFingerprint(value.options, buildContext);
    unlinkSync(value.artifact);
    assert.equal(checkFingerprint(value.options, buildContext).fresh, false);
    writeFileSync(value.artifact, "replacement binary\n", { mode: 0o700 });

    recordFingerprint(value.options, buildContext);
    writeFileSync(value.depInfo, `${value.artifact}: ${join(value.root, "missing.rs")}\n`);
    assert.equal(checkFingerprint(value.options, buildContext).fresh, false);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("updates stamps atomically without leaving a temporary file", () => {
  const value = fixture();
  try {
    recordFingerprint(value.options, buildContext);
    assert.doesNotThrow(() => JSON.parse(readFileSync(value.stamp, "utf8")));
    assert.equal(checkFingerprint(value.options, buildContext).fresh, true);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});
