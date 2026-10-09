//! Bounded structural grammar authoring, independent of world simulation and GPU APIs.
//!
//! `F` draws an analytic capsule segment. `+`/`-` yaw around local up,
//! `&`/`^` pitch around local right, and `[`/`]` save/restore turtle state.
//! Uppercase variables are inert at interpretation and must have a rewrite rule.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spatial_math::Vec3;
use std::{collections::BTreeMap, fmt, io::Read, path::Path};

/// Bounded, externally authored implicit fields, separate from structural grammars.
pub mod field_graph;

/// Maximum accepted JSON document size in bytes.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Hard ceiling for expanded symbols in one definition.
pub const MAX_SYMBOLS: usize = 65_536;
/// Hard ceiling for generated capsule segments.
pub const MAX_SEGMENTS: usize = 1_024;
/// Hard ceiling for active branch frames.
pub const MAX_STACK_DEPTH: usize = 64;
const MAX_WORK: usize = 1_000_000;
const MAX_POSITION: f64 = 1_000.0;
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;
/// Changes whenever the meaning of v1 symbols or numeric evaluation changes.
pub const COMPILER_SEMANTICS_VERSION: u32 = 1;

/// Rejection of untrusted authored input. No partial structure is returned.
#[derive(Debug)]
pub enum AuthoringError {
    /// Reading a definition file failed.
    Io(std::io::Error),
    /// Input exceeds the byte limit before parsing.
    InputTooLarge,
    /// JSON syntax or schema failed, including line and column when available.
    Json(serde_json::Error),
    /// File format version is unsupported.
    UnsupportedVersion(u32),
    /// Named field or grammar contract is invalid.
    Invalid(&'static str),
    /// A requested or actual resource budget is exceeded.
    Budget(&'static str),
    /// Finite mathematical geometry could not be produced.
    Geometry(&'static str),
    /// A referenced revision or stored snapshot does not match its exact bytes.
    RevisionMismatch,
    /// A saved snapshot format or compiler semantics is not supported.
    UnsupportedSnapshot(&'static str),
}

impl fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "definition I/O failed: {error}"),
            Self::InputTooLarge => write!(f, "definition exceeds {MAX_INPUT_BYTES} bytes"),
            Self::Json(error) => write!(f, "invalid definition JSON: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported format_version {version}; expected 1")
            }
            Self::Invalid(field) => write!(f, "invalid {field}"),
            Self::Budget(kind) => write!(f, "{kind} budget exceeded"),
            Self::Geometry(kind) => write!(f, "invalid generated geometry: {kind}"),
            Self::RevisionMismatch => write!(f, "definition revision does not match source bytes"),
            Self::UnsupportedSnapshot(kind) => write!(f, "unsupported {kind}"),
        }
    }
}

impl std::error::Error for AuthoringError {}

/// Author-selected limits, additionally constrained by hard engine ceilings.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    max_symbols: usize,
    max_stack_depth: usize,
    max_segments: usize,
    max_work: usize,
}

/// One context-free replacement. The symbol must be one uppercase ASCII letter.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    symbol: String,
    replacement: String,
}

/// Optional translation for one static instance, in world units.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceParameters {
    #[serde(default)]
    origin: [f64; 3],
}

/// Version-one declarative structural definition. It is not a scalar field.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    format_version: u32,
    id: String,
    revision: u32,
    axiom: String,
    rules: Vec<Rule>,
    iterations: u8,
    step_length: f64,
    yaw_degrees: f64,
    pitch_degrees: f64,
    branch_radius: f64,
    budgets: Budgets,
    #[serde(default)]
    instance: InstanceParameters,
}

/// SHA-256 of the exact source bytes, including whitespace and JSON key order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct DefinitionRevision([u8; 32]);

impl DefinitionRevision {
    /// Computes the byte-level revision before parsing or compiling.
    pub fn of(source: &[u8]) -> Self {
        Self(Sha256::digest(source).into())
    }

    /// Lowercase hexadecimal digest suitable for a manifest or log.
    pub fn hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// The path of an emitted symbol through at most eight parallel rewrite passes.
/// Its scope is one exact definition revision and compiler semantics version.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SegmentIdentity {
    root: u16,
    depth: u8,
    children: [u16; 8],
}

/// A revision- and semantics-scoped reference to one generated segment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SegmentReference {
    /// Exact source-byte revision.
    pub revision: DefinitionRevision,
    /// Compiler contract under which the path was produced.
    pub compiler_semantics_version: u32,
    /// Structural derivation path within that revision.
    pub identity: SegmentIdentity,
}

impl SegmentIdentity {
    fn root(index: usize) -> Result<Self, AuthoringError> {
        Ok(Self {
            root: u16::try_from(index).map_err(|_| AuthoringError::Budget("symbols"))?,
            depth: 0,
            children: [0; 8],
        })
    }

    fn child(mut self, index: usize) -> Result<Self, AuthoringError> {
        let slot = usize::from(self.depth);
        if slot >= self.children.len() {
            return Err(AuthoringError::Budget("derivation depth"));
        }
        self.children[slot] =
            u16::try_from(index).map_err(|_| AuthoringError::Budget("derivation width"))?;
        self.depth += 1;
        Ok(self)
    }

    /// Zero-based axiom byte position and replacement-child positions.
    pub fn path(self) -> (u16, Vec<u16>) {
        (self.root, self.children[..usize::from(self.depth)].to_vec())
    }
}

#[derive(Clone, Copy)]
struct DerivedSymbol {
    symbol: u8,
    identity: SegmentIdentity,
}

/// Stable segment identity is its one-based depth-first draw ordinal in a compiled definition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    /// One-based renderer draw ID. Do not persist it as structural identity.
    pub id: u32,
    /// Structural derivation path, unique within a compiled revision.
    pub identity: SegmentIdentity,
    /// Previously drawn segment in this branch, if any.
    pub parent: Option<u32>,
    /// Parent structural identity in this definition revision, if any.
    pub parent_identity: Option<SegmentIdentity>,
    /// Analytic capsule endpoint in world units.
    pub start: Vec3,
    /// Analytic capsule endpoint in world units.
    pub end: Vec3,
    /// Analytic capsule radius in world units.
    pub radius: f64,
}

/// Immutable static structure and its exact source bytes for reproducible regeneration.
#[derive(Clone, Debug)]
pub struct Structure {
    definition: Definition,
    source: Vec<u8>,
    content_revision: DefinitionRevision,
    expanded_symbols: usize,
    max_stack_used: usize,
    work: usize,
    segments: Vec<Segment>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    snapshot_version: u32,
    source_format_version: u32,
    compiler_semantics_version: u32,
    content_sha256: String,
    source: Vec<u8>,
}

/// Loads a self-contained snapshot, rejecting corruption and unknown semantics.
pub fn load_snapshot_json(bytes: &[u8]) -> Result<Structure, AuthoringError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(AuthoringError::Json)?;
    if snapshot.snapshot_version != 1 {
        return Err(AuthoringError::UnsupportedSnapshot("snapshot version"));
    }
    if snapshot.source_format_version != 1 {
        return Err(AuthoringError::UnsupportedSnapshot("source format version"));
    }
    if snapshot.compiler_semantics_version != COMPILER_SEMANTICS_VERSION {
        return Err(AuthoringError::UnsupportedSnapshot(
            "compiler semantics version",
        ));
    }
    if snapshot.source.len() > MAX_INPUT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    let actual = DefinitionRevision::of(&snapshot.source);
    if snapshot.content_sha256 != actual.hex() {
        return Err(AuthoringError::RevisionMismatch);
    }
    compile_json(&snapshot.source)
}

/// Compiles externally supplied bytes only if they match a previously recorded revision.
pub fn compile_json_expected(
    source: &[u8],
    expected: DefinitionRevision,
) -> Result<Structure, AuthoringError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    if DefinitionRevision::of(source) != expected {
        return Err(AuthoringError::RevisionMismatch);
    }
    compile_json(source)
}

impl Structure {
    /// Validated source-format version.
    pub fn format_version(&self) -> u32 {
        self.definition.format_version
    }
    /// Compiler interpretation contract used for this structure.
    pub fn compiler_semantics_version(&self) -> u32 {
        COMPILER_SEMANTICS_VERSION
    }
    /// Stable authored identifier, separate from generated segment IDs.
    pub fn id(&self) -> &str {
        &self.definition.id
    }
    /// Author-declared revision; source bytes still determine exact meaning.
    pub fn revision(&self) -> u32 {
        self.definition.revision
    }
    /// Original JSON bytes. Persist these with a preview to prevent silent file drift.
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    /// SHA-256 of the exact source bytes used to compile this structure.
    pub fn content_revision(&self) -> DefinitionRevision {
        self.content_revision
    }
    /// Converts a temporary renderer draw ID into a scoped structural reference.
    pub fn reference(&self, draw_id: u32) -> Option<SegmentReference> {
        self.segments
            .iter()
            .find(|segment| segment.id == draw_id)
            .map(|segment| SegmentReference {
                revision: self.content_revision,
                compiler_semantics_version: COMPILER_SEMANTICS_VERSION,
                identity: segment.identity,
            })
    }
    /// Finds a segment only under matching source bytes and compiler semantics.
    pub fn segment(&self, reference: SegmentReference) -> Option<&Segment> {
        (reference.revision == self.content_revision
            && reference.compiler_semantics_version == COMPILER_SEMANTICS_VERSION)
            .then(|| {
                self.segments
                    .iter()
                    .find(|segment| segment.identity == reference.identity)
            })
            .flatten()
    }
    /// Self-contained, versioned JSON snapshot with exact source bytes and integrity metadata.
    pub fn snapshot_json(&self) -> Result<Vec<u8>, AuthoringError> {
        serde_json::to_vec(&Snapshot {
            snapshot_version: 1,
            source_format_version: self.format_version(),
            compiler_semantics_version: COMPILER_SEMANTICS_VERSION,
            content_sha256: self.content_revision.hex(),
            source: self.source.clone(),
        })
        .map_err(AuthoringError::Json)
    }
    /// Generated capsule segments in deterministic depth-first order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
    /// Number of symbols after the requested rewrite iterations.
    pub fn expanded_symbols(&self) -> usize {
        self.expanded_symbols
    }
    /// Maximum active branch stack depth during interpretation.
    pub fn max_stack_used(&self) -> usize {
        self.max_stack_used
    }
    /// Count of symbols visited during rewrite and interpretation.
    pub fn work(&self) -> usize {
        self.work
    }
}

fn budget(value: usize, hard_max: usize, name: &'static str) -> Result<(), AuthoringError> {
    if value == 0 || value > hard_max {
        return Err(AuthoringError::Invalid(name));
    }
    Ok(())
}

fn symbols(value: &str, rules: &BTreeMap<char, &str>) -> Result<(), AuthoringError> {
    let mut depth = 0usize;
    for symbol in value.chars() {
        match symbol {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            ']' => return Err(AuthoringError::Invalid("branch brackets")),
            'F' | '+' | '-' | '&' | '^' => {}
            c if c.is_ascii_uppercase() && rules.contains_key(&c) => {}
            _ => return Err(AuthoringError::Invalid("grammar symbol")),
        }
    }
    if depth != 0 {
        return Err(AuthoringError::Invalid("branch brackets"));
    }
    Ok(())
}

fn validate(definition: &Definition) -> Result<BTreeMap<char, &str>, AuthoringError> {
    if definition.format_version != 1 {
        return Err(AuthoringError::UnsupportedVersion(
            definition.format_version,
        ));
    }
    let id = definition.id.as_bytes();
    if id.is_empty()
        || id.len() > 64
        || !id[0].is_ascii_lowercase()
        || !id
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
    {
        return Err(AuthoringError::Invalid("id"));
    }
    if definition.revision == 0 {
        return Err(AuthoringError::Invalid("revision"));
    }
    if definition.iterations > 8 {
        return Err(AuthoringError::Invalid("iterations"));
    }
    for (value, upper, name) in [
        (definition.step_length, 10.0, "step_length"),
        (definition.branch_radius, 1.0, "branch_radius"),
    ] {
        if !value.is_finite() || !(0.0001..=upper).contains(&value) {
            return Err(AuthoringError::Invalid(name));
        }
    }
    for (value, name) in [
        (definition.yaw_degrees, "yaw_degrees"),
        (definition.pitch_degrees, "pitch_degrees"),
    ] {
        if !value.is_finite() || !(0.0..=180.0).contains(&value) {
            return Err(AuthoringError::Invalid(name));
        }
    }
    if definition
        .instance
        .origin
        .iter()
        .any(|x| !x.is_finite() || x.abs() > MAX_POSITION)
    {
        return Err(AuthoringError::Invalid("instance.origin"));
    }
    budget(
        definition.budgets.max_symbols,
        MAX_SYMBOLS,
        "budgets.max_symbols",
    )?;
    budget(
        definition.budgets.max_stack_depth,
        MAX_STACK_DEPTH,
        "budgets.max_stack_depth",
    )?;
    budget(
        definition.budgets.max_segments,
        MAX_SEGMENTS,
        "budgets.max_segments",
    )?;
    budget(definition.budgets.max_work, MAX_WORK, "budgets.max_work")?;
    if definition.axiom.is_empty() || definition.axiom.len() > definition.budgets.max_symbols {
        return Err(AuthoringError::Invalid("axiom"));
    }
    if definition.rules.len() > 16 {
        return Err(AuthoringError::Invalid("rules"));
    }
    let mut rules = BTreeMap::new();
    for rule in &definition.rules {
        let bytes = rule.symbol.as_bytes();
        if bytes.len() != 1 || !bytes[0].is_ascii_uppercase() {
            return Err(AuthoringError::Invalid("rules.symbol"));
        }
        if rule.replacement.len() > 1024 {
            return Err(AuthoringError::Invalid("rules.replacement"));
        }
        if rules
            .insert(char::from(bytes[0]), rule.replacement.as_str())
            .is_some()
        {
            return Err(AuthoringError::Invalid("duplicate rules.symbol"));
        }
    }
    symbols(&definition.axiom, &rules)?;
    for replacement in rules.values() {
        symbols(replacement, &rules)?;
    }
    Ok(rules)
}

fn bounded_position(position: Vec3) -> Result<Vec3, AuthoringError> {
    if [position.x(), position.y(), position.z()]
        .iter()
        .any(|x| x.abs() > MAX_POSITION)
    {
        return Err(AuthoringError::Geometry(
            "position exceeds 1000 world units",
        ));
    }
    Ok(position)
}

fn combine(a: Vec3, b: Vec3, cosine: f64, sine: f64) -> Result<Vec3, AuthoringError> {
    a.checked_scale(cosine)
        .and_then(|x| b.checked_scale(sine).and_then(|y| x.checked_add(y)))
        .map_err(|_| AuthoringError::Geometry("orientation overflow"))
}

#[derive(Clone, Copy)]
struct Turtle {
    position: Vec3,
    heading: Vec3,
    right: Vec3,
    up: Vec3,
    parent: Option<u32>,
    parent_identity: Option<SegmentIdentity>,
}

impl Turtle {
    fn turn(&mut self, yaw: bool, angle: f64) -> Result<(), AuthoringError> {
        let (sine, cosine) = angle.sin_cos();
        let heading = self.heading;
        if yaw {
            self.heading = combine(heading, self.right, cosine, sine)?;
            self.right = combine(self.right, heading, cosine, -sine)?;
        } else {
            self.heading = combine(heading, self.up, cosine, sine)?;
            self.up = combine(self.up, heading, cosine, -sine)?;
        }
        Ok(())
    }
}

fn expand(
    definition: &Definition,
    rules: &BTreeMap<char, &str>,
) -> Result<(Vec<DerivedSymbol>, usize), AuthoringError> {
    let limits = definition.budgets;
    let mut expanded = definition
        .axiom
        .bytes()
        .enumerate()
        .map(|(index, symbol)| {
            Ok(DerivedSymbol {
                symbol,
                identity: SegmentIdentity::root(index)?,
            })
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;
    let mut work = 0usize;
    for _ in 0..definition.iterations {
        work = work
            .checked_add(expanded.len())
            .filter(|count| *count <= limits.max_work)
            .ok_or(AuthoringError::Budget("work"))?;
        let mut next = Vec::new();
        for derived in &expanded {
            let replacement = rules.get(&char::from(derived.symbol)).copied();
            let length = replacement.map_or(1, str::len);
            let requested = next
                .len()
                .checked_add(length)
                .filter(|count| *count <= limits.max_symbols)
                .ok_or(AuthoringError::Budget("symbols"))?;
            next.try_reserve(requested - next.len())
                .map_err(|_| AuthoringError::Budget("memory"))?;
            match replacement {
                Some(value) => {
                    for (index, symbol) in value.bytes().enumerate() {
                        next.push(DerivedSymbol {
                            symbol,
                            identity: derived.identity.child(index)?,
                        });
                    }
                }
                None => next.push(*derived),
            }
        }
        expanded = next;
    }
    work = work
        .checked_add(expanded.len())
        .filter(|count| *count <= limits.max_work)
        .ok_or(AuthoringError::Budget("work"))?;
    Ok((expanded, work))
}

fn interpret(
    definition: &Definition,
    expanded: &[DerivedSymbol],
) -> Result<(Vec<Segment>, usize), AuthoringError> {
    let limits = definition.budgets;
    let mut turtle = Turtle {
        position: Vec3::new(
            definition.instance.origin[0],
            definition.instance.origin[1],
            definition.instance.origin[2],
        )
        .map_err(|_| AuthoringError::Invalid("instance.origin"))?,
        heading: Vec3::new(0.0, 1.0, 0.0).expect("finite axis"),
        right: Vec3::new(1.0, 0.0, 0.0).expect("finite axis"),
        up: Vec3::new(0.0, 0.0, 1.0).expect("finite axis"),
        parent: None,
        parent_identity: None,
    };
    let mut stack = Vec::new();
    let mut segments = Vec::new();
    let mut max_stack_used = 0;
    let yaw = definition.yaw_degrees.to_radians();
    let pitch = definition.pitch_degrees.to_radians();
    for derived in expanded {
        match derived.symbol {
            b'F' => {
                if segments.len() >= limits.max_segments {
                    return Err(AuthoringError::Budget("segments"));
                }
                let end = turtle
                    .position
                    .checked_add(
                        turtle
                            .heading
                            .checked_scale(definition.step_length)
                            .map_err(|_| AuthoringError::Geometry("step overflow"))?,
                    )
                    .map_err(|_| AuthoringError::Geometry("step overflow"))?;
                bounded_position(end)?;
                let id = u32::try_from(segments.len() + 1)
                    .map_err(|_| AuthoringError::Budget("segments"))?;
                segments
                    .try_reserve(1)
                    .map_err(|_| AuthoringError::Budget("memory"))?;
                segments.push(Segment {
                    id,
                    identity: derived.identity,
                    parent: turtle.parent,
                    parent_identity: turtle.parent_identity,
                    start: turtle.position,
                    end,
                    radius: definition.branch_radius,
                });
                turtle.position = end;
                turtle.parent = Some(id);
                turtle.parent_identity = Some(derived.identity);
            }
            b'+' => turtle.turn(true, yaw)?,
            b'-' => turtle.turn(true, -yaw)?,
            b'&' => turtle.turn(false, pitch)?,
            b'^' => turtle.turn(false, -pitch)?,
            b'[' => {
                if stack.len() >= limits.max_stack_depth {
                    return Err(AuthoringError::Budget("stack"));
                }
                stack
                    .try_reserve(1)
                    .map_err(|_| AuthoringError::Budget("memory"))?;
                stack.push(turtle);
                max_stack_used = max_stack_used.max(stack.len());
            }
            b']' => {
                turtle = stack
                    .pop()
                    .ok_or(AuthoringError::Invalid("branch brackets"))?
            }
            _ => {} // Validated uppercase structural variable.
        }
    }
    if segments.is_empty() {
        return Err(AuthoringError::Invalid("no drawn F segments"));
    }
    Ok((segments, max_stack_used))
}

/// Parses, validates and compiles one complete JSON definition atomically.
/// No GPU, file I/O, world mutation or simulation is performed here.
pub fn compile_json(source: &[u8]) -> Result<Structure, AuthoringError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    let definition: Definition = serde_json::from_slice(source).map_err(AuthoringError::Json)?;
    let rules = validate(&definition)?;
    let (expanded, work) = expand(&definition, &rules)?;
    let (segments, max_stack_used) = interpret(&definition, &expanded)?;
    Ok(Structure {
        definition,
        source: source.to_vec(),
        content_revision: DefinitionRevision::of(source),
        expanded_symbols: expanded.len(),
        max_stack_used,
        work,
        segments,
    })
}

/// Reads one bounded, consistent byte snapshot from a definition file.
pub fn read_source_file(path: &Path) -> Result<Vec<u8>, AuthoringError> {
    let mut source = Vec::new();
    std::fs::File::open(path)
        .map_err(AuthoringError::Io)?
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut source)
        .map_err(AuthoringError::Io)?;
    if source.len() > MAX_INPUT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    Ok(source)
}

/// Reads one bounded byte snapshot, then compiles it atomically.
pub fn compile_file(path: &Path) -> Result<Structure, AuthoringError> {
    compile_json(&read_source_file(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn definition(axiom: &str, replacement: &str, iterations: u8) -> Vec<u8> {
        format!(
            r#"{{"format_version":1,"id":"test-tree","revision":1,"axiom":"{axiom}","rules":[{{"symbol":"X","replacement":"{replacement}"}}],"iterations":{iterations},"step_length":1.0,"yaw_degrees":90.0,"pitch_degrees":90.0,"branch_radius":0.1,"budgets":{{"max_symbols":1000,"max_stack_depth":16,"max_segments":100,"max_work":4000}}}}"#
        )
        .into_bytes()
    }

    #[test]
    fn rewrite_branch_order_and_recompilation_are_stable() {
        let source = definition("X", "F[+X]F[-X]X", 2);
        let a = compile_json(&source).unwrap();
        let b = compile_json(a.source()).unwrap();
        assert_eq!(a.segments(), b.segments());
        assert_eq!(a.segments().len(), 8);
        assert_eq!(a.segments()[0].parent, None);
        assert_eq!(a.segments()[1].parent, Some(1));
        assert!(
            a.segments()
                .iter()
                .all(|segment| segment.parent.is_none_or(|parent| parent < segment.id))
        );
        assert!(a.max_stack_used() >= 2);
    }

    #[test]
    fn local_yaw_pitch_and_length_change_geometry() {
        let a = compile_json(&definition("F[+F][&F]", "F", 0)).unwrap();
        let s = a.segments();
        assert_eq!(s.len(), 3);
        assert_eq!(s[1].start, s[2].start);
        assert!((s[1].end.x() - 1.0).abs() < 1e-12);
        assert!((s[2].end.z() - 1.0).abs() < 1e-12);
        let changed = String::from_utf8(definition("F[+F][&F]", "F", 0))
            .unwrap()
            .replace("\"step_length\":1.0", "\"step_length\":2.0")
            .replace("\"yaw_degrees\":90.0", "\"yaw_degrees\":45.0");
        assert_ne!(
            a.segments(),
            compile_json(changed.as_bytes()).unwrap().segments()
        );
    }

    #[test]
    fn invalid_and_exponential_inputs_fail_before_partial_output() {
        let source = definition("FX", "XX", 8);
        let expanded = compile_json(&source).unwrap();
        assert_eq!(expanded.expanded_symbols(), 257);
        for (bad, expected) in [
            (source.clone().repeat(300), "definition exceeds"),
            (definition("F]", "F", 0), "branch brackets"),
            (definition("?", "F", 0), "grammar symbol"),
            (definition("A", "F", 0), "grammar symbol"),
            (definition("X", "F[+X", 1), "branch brackets"),
            (definition("X", "[X]", 8), "no drawn"),
        ] {
            let error = compile_json(&bad).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
        let explosive = String::from_utf8(definition("X", "XXXX", 8))
            .unwrap()
            .replace("\"max_symbols\":1000", "\"max_symbols\":64");
        assert!(matches!(
            compile_json(explosive.as_bytes()),
            Err(AuthoringError::Budget("symbols"))
        ));
        let deep = String::from_utf8(definition("X", "F[[X]]", 2))
            .unwrap()
            .replace("\"max_stack_depth\":16", "\"max_stack_depth\":1");
        assert!(matches!(
            compile_json(deep.as_bytes()),
            Err(AuthoringError::Budget("stack"))
        ));
        let costly = String::from_utf8(definition("FX", "XX", 8))
            .unwrap()
            .replace("\"max_work\":4000", "\"max_work\":10");
        assert!(matches!(
            compile_json(costly.as_bytes()),
            Err(AuthoringError::Budget("work"))
        ));
    }

    #[test]
    fn malformed_parameters_versions_and_coordinates_are_rejected() {
        let source = String::from_utf8(definition("F", "F", 0)).unwrap();
        for (bad, expected) in [
            (
                source.replace("\"format_version\":1", "\"format_version\":2"),
                "unsupported",
            ),
            (
                source.replace("\"branch_radius\":0.1", "\"branch_radius\":-1"),
                "branch_radius",
            ),
            (
                source.replace("\"step_length\":1.0", "\"step_length\":0"),
                "step_length",
            ),
            (
                source.replace("\"step_length\":1.0", "\"step_length\":1e999"),
                "invalid definition JSON",
            ),
            (
                source.replace("\"max_segments\":100", "\"max_segments\":0"),
                "budgets.max_segments",
            ),
            (
                source
                    .replace("\"axiom\":\"F\"", "\"axiom\":\"FFFFFFFF\"")
                    .replace("\"max_segments\":100", "\"max_segments\":3"),
                "segments budget",
            ),
        ] {
            let error = compile_json(bad.as_bytes()).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
        let too_far = source
            .replace("\"step_length\":1.0", "\"step_length\":10.0")
            .replace(
                "\"axiom\":\"F\"",
                &format!("\"axiom\":\"{}\"", "F".repeat(101)),
            )
            .replace("\"max_segments\":100", "\"max_segments\":1024");
        assert!(matches!(
            compile_json(too_far.as_bytes()),
            Err(AuthoringError::Geometry(_))
        ));
    }

    #[test]
    fn content_revision_tracks_exact_bytes_and_expected_references() {
        assert_eq!(
            DefinitionRevision::of(b"abc").hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let source = definition("F", "F", 0);
        let mut equivalent = source.clone();
        equivalent.push(b' ');
        let first = compile_json(&source).unwrap();
        let second = compile_json(&equivalent).unwrap();
        assert_eq!(first.segments(), second.segments());
        assert_ne!(first.content_revision(), second.content_revision());
        assert!(matches!(
            compile_json_expected(&equivalent, first.content_revision()),
            Err(AuthoringError::RevisionMismatch)
        ));
        assert!(compile_json_expected(&source, first.content_revision()).is_ok());
    }

    #[test]
    fn rule_parameter_and_iteration_edits_invalidate_old_references() {
        let source = String::from_utf8(definition("X", "F[+X]F", 2)).unwrap();
        let original = compile_json(source.as_bytes()).unwrap();
        let reference = original.reference(1).unwrap();
        for edited in [
            source.replace("F[+X]F", "F[-X]F"),
            source.replace("\"step_length\":1.0", "\"step_length\":2.0"),
            source.replace("\"iterations\":2", "\"iterations\":3"),
        ] {
            let changed = compile_json(edited.as_bytes()).unwrap();
            assert_ne!(changed.content_revision(), original.content_revision());
            assert!(changed.segment(reference).is_none());
        }
    }

    #[test]
    fn derivation_paths_are_unique_even_for_repeated_geometry() {
        let same_geometry = compile_json(&definition("F[F]F", "F", 0)).unwrap();
        let segments = same_geometry.segments();
        assert_eq!(segments[1].start, segments[2].start);
        assert_eq!(segments[1].end, segments[2].end);
        assert_ne!(segments[1].identity, segments[2].identity);
        let deep = compile_json(&definition("X", "FX", 8)).unwrap();
        let repeated = compile_json(deep.source()).unwrap();
        let ids: HashSet<_> = deep
            .segments()
            .iter()
            .map(|segment| segment.identity)
            .collect();
        assert_eq!(ids.len(), deep.segments().len());
        assert_eq!(deep.segments(), repeated.segments());
        assert_eq!(deep.segments().last().unwrap().identity.path().1.len(), 8);
        let edited = String::from_utf8(deep.source().to_vec())
            .unwrap()
            .replace("\"step_length\":1.0", "\"step_length\":2.0");
        let new_revision = compile_json(edited.as_bytes()).unwrap();
        assert!(new_revision.segment(deep.reference(1).unwrap()).is_none());
        let mut wrong_semantics = deep.reference(1).unwrap();
        wrong_semantics.compiler_semantics_version += 1;
        assert!(deep.segment(wrong_semantics).is_none());
    }

    #[test]
    fn snapshot_round_trip_and_integrity_fail_closed() {
        let source = definition("F[+F]F", "F", 0);
        let original = compile_json(&source).unwrap();
        let snapshot = original.snapshot_json().unwrap();
        let restored = load_snapshot_json(&snapshot).unwrap();
        assert_eq!(restored.source(), source);
        assert_eq!(restored.content_revision(), original.content_revision());
        assert_eq!(restored.segments(), original.segments());
        let mut value: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
        value["source"][0] = serde_json::json!(42);
        assert!(matches!(
            load_snapshot_json(&serde_json::to_vec(&value).unwrap()),
            Err(AuthoringError::RevisionMismatch)
        ));
        value["compiler_semantics_version"] = serde_json::json!(999);
        assert!(matches!(
            load_snapshot_json(&serde_json::to_vec(&value).unwrap()),
            Err(AuthoringError::UnsupportedSnapshot(
                "compiler semantics version"
            ))
        ));
        value["snapshot_version"] = serde_json::json!(2);
        assert!(matches!(
            load_snapshot_json(&serde_json::to_vec(&value).unwrap()),
            Err(AuthoringError::UnsupportedSnapshot("snapshot version"))
        ));
        value["snapshot_version"] = serde_json::json!(1);
        value["source_format_version"] = serde_json::json!(2);
        assert!(matches!(
            load_snapshot_json(&serde_json::to_vec(&value).unwrap()),
            Err(AuthoringError::UnsupportedSnapshot("source format version"))
        ));
        assert!(matches!(
            load_snapshot_json(b"{}"),
            Err(AuthoringError::Json(_))
        ));
    }

    #[test]
    fn derivation_limits_accept_near_ceiling_inputs_without_collisions() {
        let symbols = String::from_utf8(definition(&format!("{}F", "X".repeat(255)), "XX", 8))
            .unwrap()
            .replace("\"max_symbols\":1000", "\"max_symbols\":65536")
            .replace("\"max_work\":4000", "\"max_work\":1000000");
        let structure = compile_json(symbols.as_bytes()).unwrap();
        assert_eq!(structure.expanded_symbols(), 65_281);
        assert_eq!(structure.segments().len(), 1);
        let primitives = String::from_utf8(definition(&"F".repeat(1024), "F", 0))
            .unwrap()
            .replace("\"max_symbols\":1000", "\"max_symbols\":65536")
            .replace("\"max_segments\":100", "\"max_segments\":1024")
            .replace("\"step_length\":1.0", "\"step_length\":0.5");
        let structure = compile_json(primitives.as_bytes()).unwrap();
        let ids: HashSet<_> = structure
            .segments()
            .iter()
            .map(|segment| segment.identity)
            .collect();
        assert_eq!(ids.len(), 1024);
    }

    #[test]
    #[ignore = "local release-profile phase timing; run explicitly with --nocapture"]
    fn authoring_phase_benchmark() {
        use std::{hint::black_box, time::Instant};
        fn p50_p95(mut samples: Vec<f64>) -> [f64; 2] {
            samples.sort_by(f64::total_cmp);
            [
                samples[samples.len() / 2],
                samples[samples.len() * 95 / 100],
            ]
        }
        let mut cases = vec![(
            "branch-a",
            include_bytes!("../../../examples/authoring/branch-a.json").to_vec(),
        )];
        let large: serde_json::Value = serde_json::from_slice(&cases[0].1).unwrap();
        let mut large = large;
        large["iterations"] = serde_json::json!(6);
        large["budgets"]["max_symbols"] = serde_json::json!(65536);
        large["budgets"]["max_segments"] = serde_json::json!(1024);
        large["budgets"]["max_work"] = serde_json::json!(1000000);
        cases.push(("728-segments", serde_json::to_vec(&large).unwrap()));
        let near_symbols = String::from_utf8(definition(&format!("{}F", "X".repeat(255)), "XX", 8))
            .unwrap()
            .replace("\"max_symbols\":1000", "\"max_symbols\":65536")
            .replace("\"max_work\":4000", "\"max_work\":1000000");
        cases.push(("65281-symbols", near_symbols.into_bytes()));
        let near_primitives = String::from_utf8(definition(&"F".repeat(1024), "F", 0))
            .unwrap()
            .replace("\"max_symbols\":1000", "\"max_symbols\":65536")
            .replace("\"max_segments\":100", "\"max_segments\":1024")
            .replace("\"step_length\":1.0", "\"step_length\":0.5");
        cases.push(("1024-segments", near_primitives.into_bytes()));
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/authoring-phase-input.json");
        for (name, source) in cases {
            std::fs::write(&path, &source).unwrap();
            let mut read = Vec::new();
            let mut parse = Vec::new();
            let mut digest = Vec::new();
            let mut rewrite_with_identity = Vec::new();
            let mut geometry = Vec::new();
            let mut counts = (0, 0);
            for _ in 0..30 {
                let start = Instant::now();
                let loaded = black_box(read_source_file(&path).unwrap());
                read.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                let definition: Definition = serde_json::from_slice(&loaded).unwrap();
                let rules = validate(&definition).unwrap();
                parse.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                black_box(DefinitionRevision::of(&loaded));
                digest.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                let (expanded, _) = expand(&definition, &rules).unwrap();
                rewrite_with_identity.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                let (segments, _) = interpret(&definition, &expanded).unwrap();
                geometry.push(start.elapsed().as_secs_f64() * 1000.0);
                counts = (expanded.len(), segments.len());
                black_box(segments);
            }
            println!(
                "{}",
                serde_json::json!({
                    "case": name,
                    "source_bytes": source.len(),
                    "symbols": counts.0,
                    "segments": counts.1,
                    "samples": 30,
                    "read_ms": p50_p95(read),
                    "parse_validate_ms": p50_p95(parse),
                    "sha256_ms": p50_p95(digest),
                    "rewrite_and_identity_ms": p50_p95(rewrite_with_identity),
                    "geometry_ms": p50_p95(geometry),
                    "derived_symbol_bytes_upper": MAX_SYMBOLS * std::mem::size_of::<DerivedSymbol>()
                })
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}
