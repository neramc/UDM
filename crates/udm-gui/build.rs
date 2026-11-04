// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Compiles the Slint UI and bundles translations into the binary.

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations(concat!(env!("CARGO_MANIFEST_DIR"), "/lang"))
        .with_default_translation_context(slint_build::DefaultTranslationContext::None)
        // Element ids and type names are needed by the headless UI tests;
        // release builds omit them.
        .with_debug_info(std::env::var("PROFILE").as_deref() != Ok("release"));
    if let Err(error) = slint_build::compile_with_config("ui/main.slint", config) {
        // Build scripts report failures by panicking; there is no other channel.
        #[allow(clippy::panic)]
        {
            panic!("failed to compile the Slint UI: {error}");
        }
    }
}
