//! Crop parity against the existing `media/edit.rs` geometry (contract §7).
//!
//! `src/lib/media/crop.ts` has three functions. Only `cropToAspect` overlaps
//! Rust geometry; it is the aspect-ratio centre crop already owned by
//! `media::edit::auto_crop` and reached through `edits_preview`. The other two
//! are UI percentage↔pixel input math with no Rust counterpart, so they are
//! NOT reimplemented here — doing so would create the second geometry
//! implementation the contract forbids. The recorded `cropFromEdges` cases are
//! checked as inputs to the existing Rust geometry instead.

#[cfg(test)]
mod tests {
    use crate::commands::edits::{Crop, EditOps, FitMode, MaskPolarity, TargetSpec};
    use crate::media::edit::{SourceInfo, auto_crop, plan};

    fn source(width: u32, height: u32) -> SourceInfo {
        SourceInfo {
            width,
            height,
            duration_ms: None,
            fps: None,
            is_video: false,
        }
    }

    fn target() -> TargetSpec {
        TargetSpec {
            preset_id: "test".into(),
            width: 0,
            height: 0,
            dimension_multiple: 16,
            mask_polarity: MaskPolarity::WhiteKeeps,
        }
    }

    fn ops_with_crop(crop: Option<Crop>) -> EditOps {
        EditOps {
            crop,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            fit: FitMode::Fit,
            trim: None,
            fps: None,
            remove_audio: false,
            mask_path: None,
            format: None,
            loop_video: false,
            pad_color: None,
        }
    }

    /// The four cases from `src/lib/media/crop.test.ts`. The third test only
    /// asserts constraints in the source; its exact crop ({450,360,100,80})
    /// was recorded by running `cropFromEdges` from the actual TypeScript
    /// source, not guessed.
    #[test]
    fn crop_test_cases_are_accepted_by_the_existing_rust_geometry() {
        let cases: Vec<(&str, Option<Crop>)> = vec![
            (
                "maps a drawn rectangle into source pixels",
                Some(Crop {
                    x: 100,
                    y: 200,
                    width: 700,
                    height: 500,
                }),
            ),
            (
                "keeps a positive size when dragging backwards",
                Some(Crop {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 80,
                }),
            ),
            (
                "clamps out-of-range edges inside the source",
                Some(Crop {
                    x: 450,
                    y: 360,
                    width: 100,
                    height: 80,
                }),
            ),
            ("returns null when the asset has no dimensions", None),
        ];

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();
        for (name, expected) in cases {
            let src = if name.contains("no dimensions") {
                source(0, 0)
            } else {
                source(1000, 800)
            };
            let planned = plan(&ops_with_crop(expected.clone()), &target(), &src);
            compared += 1;
            match planned {
                Ok(plan) if plan.crop == expected => {}
                Ok(plan) => differences.push(format!(
                    "{name}: expected crop {expected:?}, geometry produced {:?}",
                    plan.crop
                )),
                Err(error) => {
                    differences.push(format!("{name}: geometry rejected crop: {error:?}"))
                }
            }
        }

        assert!(
            differences.is_empty(),
            "crop: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "crop: {compared} inputs compared, {} differences (routed through media::edit::plan / edits_preview)",
            differences.len()
        );
    }

    #[test]
    fn reports_the_exact_gap_that_cannot_be_ported_without_a_second_geometry() {
        // cropToAspect ↔ auto_crop: same centre-crop idea, different centring
        // on odd differences. Source uses Math.round((w - cw) / 2); Rust uses
        // integer division (floor). 1001x800 at ratio 1: TS x=101, Rust x=100.
        let rust_crop = auto_crop(1001, 800, 1, 1).expect("centre crop");
        assert_eq!(rust_crop.x, 100);
        assert_eq!(rust_crop.width, 800);
        assert_eq!(rust_crop.height, 800);
        println!(
            "crop GAP: cropToAspect(1001x800, 1) -> x=101 in TS, x={} in auto_crop (Math.round vs integer division)",
            rust_crop.x
        );
        println!(
            "crop GAP: edgesFromCrop / cropFromEdges have no media/edit.rs counterpart; \
             their output is passed unchanged as EditOps.crop to edits_preview, so no \
             second geometry implementation is introduced"
        );
    }
}
