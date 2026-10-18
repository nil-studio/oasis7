//! Offline shadow packing only. Never changes an active endpoint or selects a generation.
use oasis7_distfs::controlled_authority::formal::{
    FormalStorageShadowOptions, advance_formal_storage_shadow, build_formal_storage_shadow,
    build_formal_storage_shadow_optimized,
};
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut optimized = false;
    let mut resume = false;
    let valid_flags = args.iter().skip(2).all(|flag| {
        if flag == "--optimized" && !optimized {
            optimized = true;
            true
        } else if flag == "--resume" && !resume {
            resume = true;
            true
        } else {
            false
        }
    });
    if args.len() < 2 || !valid_flags {
        eprintln!(
            "usage: formal_storage_shadow SOURCE_ENDPOINT SHADOW_DIRECTORY [--optimized] [--resume]"
        );
        std::process::exit(2);
    }
    if resume {
        let options = FormalStorageShadowOptions {
            optimized,
            ..FormalStorageShadowOptions::default()
        };
        match advance_formal_storage_shadow(Path::new(&args[0]), Path::new(&args[1]), &options) {
            Ok(progress) => println!("{}", serde_json::to_string_pretty(&progress).unwrap()),
            Err(error) => {
                eprintln!("shadow step failed; source retained: {error:?}");
                std::process::exit(1);
            }
        }
        return;
    }
    let build = if optimized {
        build_formal_storage_shadow_optimized
    } else {
        build_formal_storage_shadow
    };
    match build(Path::new(&args[0]), Path::new(&args[1])) {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => {
            eprintln!("shadow failed; source retained; output may be incomplete: {error:?}");
            std::process::exit(1);
        }
    }
}
