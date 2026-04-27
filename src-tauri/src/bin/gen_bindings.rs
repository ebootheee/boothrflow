//! Emits TypeScript bindings (commands + types) for the frontend to import.
//!
//! Output: `../src/lib/ipc/bindings.ts`. Re-run after editing any
//! `#[tauri::command]` signature or `#[derive(specta::Type)]` struct.

use specta_typescript::Typescript;
use tauri_specta::{collect_commands, Builder};

use boothrflow_lib::commands::dictate_once;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![dictate_once]);

    builder.export(
        Typescript::default()
            .header("// AUTO-GENERATED. Do not edit. Run `pnpm gen` to regenerate.\n"),
        "../src/lib/ipc/bindings.ts",
    )?;

    println!("wrote ../src/lib/ipc/bindings.ts");
    Ok(())
}
