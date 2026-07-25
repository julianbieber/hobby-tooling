//! The three worked examples must parse with zero diagnostics (acceptance
//! criterion), plus a smoke test for the `--` module separator. These fixtures
//! are the same specs the `mml-extract` skill ships as its worked examples, so
//! the docs and the parser stay in agreement.

use mml_parser::analyze;

fn assert_clean(name: &str, src: &str) {
    let (_doc, diags) = analyze(src);
    assert!(
        diags.is_empty(),
        "{name} should have no diagnostics, got: {diags:#?}"
    );
}

#[test]
fn mesh_terrain_chunks_is_clean() {
    assert_clean(
        "mesh-terrain-chunks",
        include_str!("fixtures/examples/mesh-terrain-chunks.hmml"),
    );
}

#[test]
fn get_map_tile_is_clean() {
    assert_clean(
        "get-map-tile",
        include_str!("fixtures/examples/get-map-tile.hmml"),
    );
}

#[test]
fn reload_shader_batch_is_clean() {
    assert_clean(
        "reload-shader-batch",
        include_str!("fixtures/examples/reload-shader-batch.hmml"),
    );
}

#[test]
fn examples_produce_expected_structure() {
    let src = include_str!("fixtures/examples/get-map-tile.hmml");
    let (doc, _) = analyze(src);
    // TileHandler, TileStore, TileRenderer => 3 modules.
    assert_eq!(doc.modules.len(), 3);
    // TileFormat (enum), TileRequest, Tile => 3 defs.
    assert_eq!(doc.data.types.len(), 3);
    assert_eq!(doc.data.input.as_ref().unwrap().ident.value, "TileRequest");
    assert_eq!(doc.data.output.as_ref().unwrap().ident.value, "Tile");
    assert_eq!(doc.invariants.len(), 3);
}

#[test]
fn double_hyphen_separator_is_accepted() {
    let src = "\
system: s
unit: u
kind: job

## modules
Foo -- does a thing @ path/to/foo.rs

## data
in: String
out: String

## behavior
- do it

## invariants
- always true
";
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    assert_eq!(doc.modules[0].name.value, "Foo");
    assert_eq!(doc.modules[0].responsibility.value, "does a thing");
    assert_eq!(
        doc.modules[0].path.as_ref().unwrap().value,
        "path/to/foo.rs"
    );
}

#[test]
fn lowercase_enum_variants_are_accepted() {
    let src = include_str!("fixtures/examples/reload-shader-batch.hmml");
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    let outcome = doc
        .data
        .types
        .iter()
        .find(|d| d.name().value == "ReloadOutcome")
        .expect("ReloadOutcome enum");
    match outcome {
        mml_parser::ast::TypeDef::Enum { variants, .. } => {
            let names: Vec<_> = variants.iter().map(|v| v.value.as_str()).collect();
            assert_eq!(names, ["swapped", "unchanged", "rejected", "skipped"]);
        }
        _ => panic!("ReloadOutcome should be an enum"),
    }
}

#[test]
fn endpoint_method_and_param_source_parse_clean() {
    let src = "\
system: s
unit: u
kind: endpoint
method: GET

## modules
Ctrl — handles it @ c.scala

## data
struct Req {
  contract_id: String @path,
  q: String @query,
}
in: Req
out: String

## behavior
- do it

## invariants
- always
";
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    assert_eq!(doc.header.method.as_ref().unwrap().value, "GET");
    match &doc.data.types[0] {
        mml_parser::ast::TypeDef::Struct { fields, .. } => {
            assert_eq!(fields[0].source.as_ref().unwrap().value, "path");
            assert_eq!(fields[1].source.as_ref().unwrap().value, "query");
        }
        _ => panic!("Req should be a struct"),
    }
}

#[test]
fn cross_spec_refs_parse_clean() {
    let src = include_str!("fixtures/examples/cross-spec-refs.hmml");
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");

    // Module with both `@ path` and `@ spec` (em-dash separator pins the
    // byte-offset math): the annotation is split off the path cleanly.
    let dispatcher = &doc.modules[0];
    assert_eq!(
        dispatcher.path.as_ref().unwrap().value,
        "src/watch/dispatcher.rs"
    );
    let m_spec = dispatcher.spec.as_ref().unwrap();
    assert_eq!(m_spec.value, "terrain-erosion/mesh-terrain-chunks");
    assert_eq!(
        &src[m_spec.span.start..m_spec.span.end],
        "terrain-erosion/mesh-terrain-chunks"
    );

    // Module with a bare (same-system) ref and no path.
    let writer = &doc.modules[1];
    assert_eq!(writer.path, None);
    assert_eq!(writer.spec.as_ref().unwrap().value, "write-reload-results");
    assert_eq!(writer.responsibility.value, "records reload outcomes");

    // A control-flow step keeps its kind despite the trailing annotation.
    let for_each = &doc.behavior[1];
    assert_eq!(for_each.kind, mml_parser::ast::StepKind::ForEach);
    assert_eq!(for_each.text.value, "for each asset:");
    assert_eq!(
        for_each.spec.as_ref().unwrap().value,
        "terrain-erosion/mesh-terrain-chunks"
    );

    // A nested prose step carries its ref; the text excludes the annotation.
    let nested = &doc.behavior[2].children[0];
    assert_eq!(nested.text.value, "hand the asset to the batch retry flow");
    assert_eq!(nested.spec.as_ref().unwrap().value, "reload-shader-batch");

    // Invariant ref, with exact span text.
    let inv_spec = doc.invariants[0].spec.as_ref().unwrap();
    assert_eq!(inv_spec.value, "shader-runner/reload-shader-batch");
    assert_eq!(
        &src[inv_spec.span.start..inv_spec.span.end],
        "shader-runner/reload-shader-batch"
    );
    assert_eq!(
        doc.invariants[0].text.value,
        "every failed reload reaches the batch flow"
    );

    // split_spec_ref: qualified vs bare.
    assert_eq!(
        mml_parser::ast::split_spec_ref("terrain-erosion/mesh-terrain-chunks"),
        (Some("terrain-erosion"), "mesh-terrain-chunks")
    );
    assert_eq!(
        mml_parser::ast::split_spec_ref("reload-shader-batch"),
        (None, "reload-shader-batch")
    );
}

#[test]
fn shared_data_imports_parse_clean() {
    let src = include_str!("fixtures/examples/shared-data-imports.hmml");
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");

    // Two `use` lines: a qualified ref importing two types, then a bare ref.
    assert_eq!(doc.data.imports.len(), 2);
    let first = &doc.data.imports[0];
    assert_eq!(first.spec.value, "terrain-erosion/mesh-terrain-chunks");
    assert_eq!(
        &src[first.spec.span.start..first.spec.span.end],
        "terrain-erosion/mesh-terrain-chunks"
    );
    let names: Vec<_> = first.types.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(names, ["ChunkMesh", "ChunkCoord"]);
    // Exact span of the second imported type pins the comma-split byte math.
    assert_eq!(
        &src[first.types[1].span.start..first.types[1].span.end],
        "ChunkCoord"
    );

    let second = &doc.data.imports[1];
    assert_eq!(second.spec.value, "get-map-tile");
    assert_eq!(
        second
            .types
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>(),
        ["TileFormat"]
    );

    // The imported names resolve: `ChunkMesh`/`ChunkCoord`/`TileFormat` are used
    // in the local struct, `in:`, and `out:` without an `undefined-type` error
    // (the empty `diags` above already asserts this — this is the point of the
    // feature).
    assert_eq!(doc.data.types.len(), 1);
}

#[test]
fn where_bindings_parse_clean() {
    let src = "\
system: s
unit: u
kind: workflow

## modules
W — worker @ w.py

## data
in: String
out: String

## behavior
- spawn X children
where:
  X = 4 @ TERRAIN_MESH_WORKERS
  N = 10

## invariants
- at most X in flight
";
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    assert_eq!(doc.behavior.len(), 1, "where: must not swallow the bullets");
    assert_eq!(doc.bindings.len(), 2);
    assert_eq!(doc.bindings[0].symbol.value, "X");
    assert_eq!(doc.bindings[0].value.value, "4");
    assert_eq!(
        doc.bindings[0].config.as_ref().unwrap().value,
        "TERRAIN_MESH_WORKERS"
    );
    assert_eq!(doc.bindings[1].symbol.value, "N");
    assert_eq!(doc.bindings[1].value.value, "10");
    assert_eq!(doc.bindings[1].config, None);
}

// ---- Bevy dialect examples --------------------------------------------------

#[test]
fn enemy_waves_plugin_is_clean() {
    assert_clean(
        "enemy-waves",
        include_str!("fixtures/examples/enemy-waves.hmml"),
    );
}

#[test]
fn spawn_enemies_system_is_clean() {
    assert_clean(
        "spawn-enemies",
        include_str!("fixtures/examples/spawn-enemies.hmml"),
    );
}

#[test]
fn plugin_produces_expected_structure() {
    use mml_parser::ast::{Kind, Role, TypeDef};

    let src = include_str!("fixtures/examples/enemy-waves.hmml");
    let (doc, _) = analyze(src);
    assert_eq!(doc.header.kind.as_ref().unwrap().value, Kind::Plugin);

    // Three world sections, each carrying its role.
    assert_eq!(doc.components.role, Role::Component);
    let comps: Vec<&str> = doc
        .components
        .types
        .iter()
        .map(|t| t.name().value.as_str())
        .collect();
    assert_eq!(comps, ["Wave", "EnemyHealth", "Spawner"]);
    // `marker Spawner` is a zero-sized component, not a struct.
    assert!(matches!(doc.components.types[2], TypeDef::Marker { .. }));
    assert_eq!(doc.resources.types.len(), 2);
    assert_eq!(doc.events.types.len(), 1);

    // A plugin uses no `## data`, so there is no input or output type at all.
    assert!(doc.data.types.is_empty());
    assert!(doc.data.input.is_none());
    assert!(doc.data.output.is_none());

    // Systems, with their access blocks.
    let names: Vec<&str> = doc.systems.iter().map(|s| s.name.value.as_str()).collect();
    assert_eq!(
        names,
        ["tick_wave_timer", "spawn_enemies", "detect_cleared"]
    );

    let spawn = &doc.systems[1];
    assert_eq!(spawn.access.schedule.as_ref().unwrap().value, "FixedUpdate");
    assert_eq!(spawn.access.after[0].value, "tick_wave_timer");
    assert_eq!(spawn.access.run_if.as_ref().unwrap().value, "WaveState.due");
    let query = spawn.access.query.as_ref().unwrap();
    let with: Vec<&str> = query.with.iter().map(|w| w.value.as_str()).collect();
    assert_eq!(with, ["Transform", "Spawner"]);
    assert!(query.without.is_empty());
    let spawns: Vec<&str> = spawn
        .access
        .spawns
        .iter()
        .map(|s| s.value.as_str())
        .collect();
    assert_eq!(spawns, ["EnemyHealth", "Wave"]);
    // The system is specced in full elsewhere.
    assert_eq!(spawn.spec.as_ref().unwrap().value, "spawn-enemies");
    // The first system's `@ path` is a source reference, not a schedule.
    assert_eq!(
        doc.systems[0].path.as_ref().unwrap().value,
        "src/waves/timer.rs"
    );

    assert_eq!(doc.behavior.len(), 3);
    assert_eq!(doc.bindings.len(), 1);
    assert_eq!(doc.invariants.len(), 4);
}

#[test]
fn system_kind_produces_a_single_access_block() {
    use mml_parser::ast::Kind;

    let src = include_str!("fixtures/examples/spawn-enemies.hmml");
    let (doc, _) = analyze(src);
    assert_eq!(doc.header.kind.as_ref().unwrap().value, Kind::System);
    assert!(doc.systems.is_empty());

    let access = doc.access.as_ref().expect("## access block");
    assert_eq!(access.schedule.as_ref().unwrap().value, "FixedUpdate");
    assert_eq!(access.sets[0].value, "WaveSpawning");
    let query = access.query.as_ref().unwrap();
    let with: Vec<&str> = query.with.iter().map(|w| w.value.as_str()).collect();
    let without: Vec<&str> = query.without.iter().map(|w| w.value.as_str()).collect();
    assert_eq!(with, ["Transform", "Spawner"]);
    assert_eq!(without, ["SpawnBudget"]);

    // Shapes it does not own are imported, with the section assigning the role.
    assert_eq!(doc.components.imports[0].types.len(), 3);
    assert_eq!(doc.resources.imports[0].spec.value, "enemy-waves");
    let roles = doc.roles();
    assert_eq!(roles["Spawner"], mml_parser::ast::Role::Component);
    assert_eq!(roles["WaveConfig"], mml_parser::ast::Role::Resource);
}

#[test]
fn source_paths_include_system_entries() {
    let src = include_str!("fixtures/examples/enemy-waves.hmml");
    let (doc, _) = analyze(src);
    let paths: Vec<&str> = mml_parser::validate::collect_source_paths(&doc)
        .into_iter()
        .map(|p| p.value.as_str())
        .collect();
    // Two header paths, one module `@ path`, one system `@ path`.
    assert!(paths.contains(&"src/waves/timer.rs"), "{paths:?}");
    assert!(paths.contains(&"src/waves/mod.rs"), "{paths:?}");
}

#[test]
fn spec_refs_include_system_entries() {
    let src = include_str!("fixtures/examples/enemy-waves.hmml");
    let (doc, _) = analyze(src);
    let refs: Vec<&str> = mml_parser::validate::collect_spec_refs(&doc)
        .into_iter()
        .map(|r| r.value.as_str())
        .collect();
    assert_eq!(refs, ["spawn-enemies"]);
}
