// ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
// ┃ ██████ ██████ ██████       █      █      █      █      █ █▄  ▀███ █       ┃
// ┃ ▄▄▄▄▄█ █▄▄▄▄▄ ▄▄▄▄▄█  ▀▀▀▀▀█▀▀▀▀▀ █ ▀▀▀▀▀█ ████████▌▐███ ███▄  ▀█ █ ▀▀▀▀▀ ┃
// ┃ █▀▀▀▀▀ █▀▀▀▀▀ █▀██▀▀ ▄▄▄▄▄ █ ▄▄▄▄▄█ ▄▄▄▄▄█ ████████▌▐███ █████▄   █ ▄▄▄▄▄ ┃
// ┃ █      ██████ █  ▀█▄       █ ██████      █      ███▌▐███ ███████▄ █       ┃
// ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
// ┃ Copyright (c) 2017, the Perspective Authors.                              ┃
// ┃ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ ┃
// ┃ This file is part of the Perspective library, distributed under the terms ┃
// ┃ of the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). ┃
// ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

import { execSync } from "child_process";
import { build } from "@finos/perspective-esbuild-plugin/build.js";
import { PerspectiveEsbuildPlugin } from "@finos/perspective-esbuild-plugin";
import { NodeModulesExternal } from "@finos/perspective-esbuild-plugin/external.js";
import * as fs from "node:fs";

import cpy from "cpy";

const IS_DEBUG =
    !!process.env.PSP_DEBUG || process.argv.indexOf("--debug") >= 0;

const INHERIT = {
    stdio: "inherit",
    stderr: "inherit",
};

function get_host() {
    return /host\: (.+?)$/gm.exec(execSync(`rustc -vV`).toString())[1];
}
async function build_all() {
    // Rust compile-time metadata
    // console.log(
    //     `cargo build -p perspective-viewer --bin metadata --target=${get_host()} ${
    //         IS_DEBUG ? "" : "--release"
    //     }`
    // );
    execSync(
        `cargo build -p perspective-viewer --bin perspective-viewer-metadata --target=${get_host()}`,
        INHERIT
    );

    // TODO Fix this shit
    const docs = execSync(
        `../target/${get_host()}/debug/perspective-viewer-metadata --docs`
    );

    execSync(
        `TS_RS_EXPORT_DIR='./src/ts/ts-rs' ../target/${get_host()}/debug/perspective-viewer-metadata`
    );

    fs.writeFileSync("./exprtk.md", docs.toString());
    if (!fs.existsSync("./dist/pkg")) {
        fs.mkdirSync("./dist/pkg", { recursive: true });
    }

    // fs.writeFileSync("./dist/pkg/rust_types.d.ts", types.toString());
    execSync(
        `cargo bundle --target=${get_host()} -- perspective_viewer ${
            IS_DEBUG ? "" : "--release"
        }`,
        INHERIT
    );

    // JavaScript
    const BUILD = [
        {
            entryPoints: ["src/ts/perspective-viewer.ts"],
            format: "esm",
            plugins: [NodeModulesExternal()],
            external: ["*.wasm"],
            outdir: "dist/esm",
        },
        {
            entryPoints: ["src/ts/perspective-viewer.ts"],
            format: "esm",
            plugins: [PerspectiveEsbuildPlugin({ wasm: { inline: true } })],
            outfile: "dist/esm/perspective-viewer.inline.js",
        },
        {
            entryPoints: ["src/ts/perspective-viewer.ts"],
            format: "esm",
            plugins: [PerspectiveEsbuildPlugin()],
            splitting: true,
            outdir: "dist/cdn",
        },
    ];

    await Promise.all(BUILD.map(build)).catch(() => process.exit(1));

    // This is dumb.  `splitting` param for `esbuild` outputs a `__require`/
    // `__exports`/`__esModule` polyfill and does not tree-shake it;  this <1kb
    // file blocks downloading of the wasm asset until after alot of JavaScript has
    // parsed due to this multi-step download+eval.  Luckily `esbuild` is quite fast
    // enough to just run another build to inline this one file `chunk.js`.
    const POSTBUILD = [
        {
            entryPoints: ["dist/cdn/perspective-viewer.js"],
            format: "esm",
            plugins: [NodeModulesExternal()],
            external: ["*.wasm", "*.worker.js", "*.main.js"],
            outdir: "dist/cdn",
            allowOverwrite: true,
        },
    ];

    await Promise.all(POSTBUILD.map(build)).catch(() => process.exit(1));

    // Typecheck
    execSync("npx tsc --project tsconfig.json", INHERIT);

    // legacy compat
    await cpy("target/themes/*", "dist/css");
}

build_all();
