// SPDX-License-Identifier: Apache-2.0

//! Build script for `snf-radar`.
//!
//! Only does real work under the `sdk` feature: it compiles the `cxx` bridge
//! (`src/ffi.rs`) together with the C++ shim (`cxx/src/mmwave_shim.cpp`) that
//! wraps the TI mmWave SDK's TLV parser. Without `sdk`, the crate is pure Rust
//! and this script is a no-op, so `cargo check` needs neither a C++ toolchain
//! nor the vendor SDK.

fn main() {
    #[cfg(feature = "sdk")]
    {
        use std::{env, path::PathBuf};

        let mut build = cxx_build::bridge("src/ffi.rs");
        build
            .file("cxx/src/mmwave_shim.cpp")
            .include("cxx/include")
            .std("c++17");

        // The TI mmWave SDK provides the actual frame/TLV parser. Point
        // MMWAVE_SDK_PATH at the SDK root so its headers resolve; the specific
        // components to link are catalogued in
        // `references/mmwave_sdk_software_manifest.html`.
        if let Ok(sdk) = env::var("MMWAVE_SDK_PATH") {
            let sdk = PathBuf::from(sdk);
            build.include(sdk.join("packages"));
            println!("cargo:rerun-if-env-changed=MMWAVE_SDK_PATH");
        } else {
            println!(
                "cargo:warning=snf-radar built with `sdk` but MMWAVE_SDK_PATH is unset; \
                 the C++ shim will compile against its stub definitions only"
            );
        }

        build.compile("snf_radar_cxx");

        println!("cargo:rerun-if-changed=src/ffi.rs");
        println!("cargo:rerun-if-changed=cxx/src/mmwave_shim.cpp");
        println!("cargo:rerun-if-changed=cxx/include/mmwave_shim.h");
    }
}
