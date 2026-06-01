const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yaml");
const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");

#[test]
fn ci_uses_rustc_new_enough_for_bevy_rc2() {
    assert!(
        CI_WORKFLOW.contains("toolchain: nightly-2026-04-06"),
        "CI must use a nightly new enough to compile Bevy 0.19.0-rc.2"
    );
    assert!(
        CI_WORKFLOW.contains("bevy_lint_toolchain: nightly-2026-03-05"),
        "Bevy Lint must use a nightly that supports Bevy 0.19.0-rc.2 and its rustc-private APIs"
    );
    assert!(
        !CI_WORKFLOW.contains("nightly-2025-06-26"),
        "CI must not use the old nightly that is too old for Bevy 0.19.0-rc.2"
    );
}

#[test]
fn ci_uses_rc2_compatible_bevy_lint() {
    let bevy_lints_job = CI_WORKFLOW
        .split("  bevy-lints:")
        .nth(1)
        .and_then(|job_and_rest| job_and_rest.split("  tests:").next())
        .expect("CI workflow should contain a Bevy lints job before the tests job");

    assert!(
        CI_WORKFLOW.contains("bevy_lint_rev: 76e0288e719be1d0de25818cb22e97f73c0ca44d"),
        "Bevy Lint must be pinned to the verified revision compatible with Bevy 0.19.0-rc.2"
    );
    assert!(
        !CI_WORKFLOW.contains("949b808e6bbf11be0f61b236fbbc4e35854698af"),
        "CI must not install the old Bevy Lint revision"
    );
    assert!(
        !bevy_lints_job.contains("-Zcodegen-backend=cranelift"),
        "Bevy Lint's rustc driver cannot use the cranelift backend with the verified toolchain"
    );
}

#[test]
fn manifest_pins_bevy_rc2() {
    for dependency in [
        "bevy_app",
        "bevy_asset",
        "bevy_core_pipeline",
        "bevy_color",
        "bevy_derive",
        "bevy_image",
        "bevy_ecs",
        "bevy_log",
        "bevy_math",
        "bevy_platform",
        "bevy_reflect",
        "bevy_render",
        "bevy_camera",
        "bevy_time",
        "bevy_transform",
        "bevy",
    ] {
        let expected = format!("{dependency} = {{ version = \"0.19.0-rc.2\" }}");
        assert!(
            CARGO_MANIFEST.contains(&expected),
            "{dependency} must stay pinned to Bevy 0.19.0-rc.2"
        );
    }
}
