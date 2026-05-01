//! Regenerate `src/lib/ipc/bindings.ts` from the current command surface.
//!
//! Tauri-specta's debug-mode auto-export requires a running Tauri app, which
//! creates a chicken-and-egg for the FE: you need the bindings to type-check
//! the FE, but generating them traditionally needs `tauri dev` to launch.
//! This example sidesteps that — it builds the same `SpectaBuilder` we use
//! at runtime and just calls `.export()` directly. No window, no plugins,
//! no settings store. Run via `pnpm gen`.

use boothrflow_lib::build_specta;
use specta_typescript::Typescript;

fn main() {
    let header = "// AUTO-GENERATED — do not edit. Source: tauri-specta in src-tauri/src/lib.rs.\n\
         // Regenerate via `pnpm gen` (or `cargo run --example export_bindings`).\n";
    let target = "../src/lib/ipc/bindings.ts";
    build_specta()
        .export(Typescript::default().header(header), target)
        .expect("export tauri-specta TS bindings");
    println!("specta: wrote {target}");
}
