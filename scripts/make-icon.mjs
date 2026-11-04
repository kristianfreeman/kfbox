#!/usr/bin/env node
// Generates kfbox's app icon from its formula, a Lissajous figure
// x = sin(3t + pi/2), y = sin(2t), drawn as one closed stroke in "Ink".
//
//   node scripts/make-icon.mjs
//
// Writes into src-tauri/icons/:
//   AppIcon.icon/   Icon Composer document (macOS 26 Liquid Glass): the
//                   background as the document's fill and the figure as an
//                   SVG layer, each with a dark-appearance specialization.
//   Assets.car      that document compiled by actool, checked in so a build
//                   needs no Xcode 26. Listed in bundle.icon, Tauri copies it
//                   to Contents/Resources and sets CFBundleIconName from it.
//   icon.icns, icon.png, icon.ico, 32x32.png, 128x128.png, 128x128@2x.png
//                   the flat fallback for older macOS (and the window/dock
//                   icon in dev), rasterised by scripts/make-icon.swift and
//                   cut into sizes by `tauri icon`.
//
// Needs macOS with Xcode (actool, swift). Pass --previews <dir> to also render
// every appearance with Icon Composer's ictool, for a look.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const icons = join(root, "src-tauri/icons");
const doc = join(icons, "AppIcon.icon");

// Geometry, as fractions of the icon tile (the full 1024 canvas for the
// Liquid Glass layer, the 824 px squircle for the flat icon).
const RADIUS = 0.27;
const STROKE = 0.055;
const SAMPLES = 2000;

const ink = {
  light: { stroke: ["#2a2a33", "#0c0c10"], fill: ["#ffffff", "#e4e4ea"] },
  dark: { stroke: ["#f4f4f6", "#c8c8d0"], fill: ["#1c1c22", "#0c0c10"] },
};

function figurePath(size) {
  const c = size / 2;
  const r = size * RADIUS;
  const pts = [];
  for (let i = 0; i < SAMPLES; i++) {
    const t = (i / SAMPLES) * 2 * Math.PI;
    const x = c + r * Math.sin(3 * t + Math.PI / 2);
    const y = c + r * Math.sin(2 * t);
    pts.push(`${x.toFixed(2)} ${y.toFixed(2)}`);
  }
  return `M${pts.join("L")}Z`;
}

function figureSvg([top, bottom]) {
  const size = 1024;
  const c = size / 2;
  const r = size * RADIUS;
  const w = size * STROKE;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  <defs>
    <linearGradient id="ink" gradientUnits="userSpaceOnUse" x1="${c}" y1="${c - r - w / 2}" x2="${c}" y2="${c + r + w / 2}">
      <stop offset="0" stop-color="${top}"/>
      <stop offset="1" stop-color="${bottom}"/>
    </linearGradient>
  </defs>
  <path d="${figurePath(size)}" fill="none" stroke="url(#ink)" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
`;
}

// Icon Composer colours: "srgb:r,g,b,a" with five decimals.
function srgb(hex) {
  const n = parseInt(hex.slice(1), 16);
  const ch = [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff].map((v) => (v / 255).toFixed(5));
  return `srgb:${ch.join(",")},1.00000`;
}

function gradient([top, bottom]) {
  return {
    "linear-gradient": [srgb(top), srgb(bottom)],
    orientation: { start: { x: 0.5, y: 0 }, stop: { x: 0.5, y: 1 } },
  };
}

function writeDocument() {
  rmSync(doc, { recursive: true, force: true });
  mkdirSync(join(doc, "Assets"), { recursive: true });
  writeFileSync(join(doc, "Assets/figure.svg"), figureSvg(ink.light.stroke));
  writeFileSync(join(doc, "Assets/figure-dark.svg"), figureSvg(ink.dark.stroke));
  const json = {
    "fill-specializations": [
      { value: gradient(ink.light.fill) },
      { appearance: "dark", value: gradient(ink.dark.fill) },
    ],
    groups: [
      {
        layers: [
          {
            name: "figure",
            "image-name-specializations": [
              { value: "figure.svg" },
              { appearance: "dark", value: "figure-dark.svg" },
              { appearance: "tinted", value: "figure-dark.svg" },
            ],
            glass: true,
          },
        ],
        shadow: { kind: "neutral", opacity: 0.5 },
        translucency: { enabled: false, value: 0 },
      },
    ],
    "supported-platforms": { squares: ["macOS"] },
  };
  writeFileSync(join(doc, "icon.json"), JSON.stringify(json, null, 2) + "\n");
}

function compile(work) {
  const out = join(work, "car");
  mkdirSync(out);
  // prettier-ignore
  execFileSync("xcrun", [
    "actool", doc, "--compile", out, "--app-icon", "AppIcon",
    "--enable-on-demand-resources", "NO", "--development-region", "en",
    "--target-device", "mac", "--platform", "macosx",
    "--minimum-deployment-target", "11.0",
    "--output-partial-info-plist", join(out, "partial.plist"),
  ], { stdio: ["ignore", "ignore", "inherit"] });
  copyFileSync(join(out, "Assets.car"), join(icons, "Assets.car"));
}

function flat(work) {
  const png = join(work, "flat-1024.png");
  execFileSync("swift", [join(root, "scripts/make-icon.swift"), png, "1024", String(RADIUS), String(STROKE)], {
    stdio: "inherit",
  });
  const out = join(work, "tauri");
  execFileSync("pnpm", ["-s", "tauri", "icon", png, "-o", out], { cwd: root, stdio: ["ignore", "ignore", "inherit"] });
  for (const f of ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.icns", "icon.ico"]) {
    copyFileSync(join(out, f), join(icons, f));
  }
  return png;
}

function previews(dir, flatPng) {
  mkdirSync(dir, { recursive: true });
  const ictool = "/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool";
  for (const rendition of ["Default", "Dark", "ClearLight", "ClearDark", "TintedLight", "TintedDark"]) {
    for (const px of [1024, 32]) {
      const extra = rendition.startsWith("Tinted") ? ["--tint-color", "0.6", "--tint-strength", "0.75"] : [];
      // prettier-ignore
      execFileSync(ictool, [
        doc, "--export-image", "--output-file", join(dir, `${rendition}-${px}.png`),
        "--platform", "macOS", "--rendition", rendition,
        "--width", String(px), "--height", String(px), "--scale", "1", ...extra,
      ], { stdio: ["ignore", "ignore", "inherit"] });
    }
  }
  copyFileSync(flatPng, join(dir, "flat-1024.png"));
  execFileSync("sips", ["-z", "32", "32", flatPng, "--out", join(dir, "flat-32.png")], { stdio: "ignore" });
}

const work = mkdtempSync(join(tmpdir(), "kfbox-icon-"));
try {
  writeDocument();
  compile(work);
  const png = flat(work);
  const i = process.argv.indexOf("--previews");
  if (i > 0) previews(resolve(process.argv[i + 1]), png);
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log("icons written to src-tauri/icons");
