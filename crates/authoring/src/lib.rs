//! Bounded structural grammar authoring, independent of world simulation and GPU APIs.
//!
//! `F` draws an analytic capsule segment. `+`/`-` yaw around local up,
//! `&`/`^` pitch around local right, and `[`/`]` save/restore turtle state.
//! Uppercase variables are inert at interpretation and must have a rewrite rule.

use serde::Deserialize;
use spatial_math::Vec3;
use std::{collections::BTreeMap, fmt, io::Read, path::Path};

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

/// Stable segment identity is its one-based depth-first draw ordinal in a compiled definition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    /// One-based generated identity, stable for identical source bytes and compiler version.
    pub id: u32,
    /// Previously drawn segment in this branch, if any.
    pub parent: Option<u32>,
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
    expanded_symbols: usize,
    max_stack_used: usize,
    work: usize,
    segments: Vec<Segment>,
}

impl Structure {
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

/// Parses, validates and compiles one complete JSON definition atomically.
/// No GPU, file I/O, world mutation or simulation is performed here.
pub fn compile_json(source: &[u8]) -> Result<Structure, AuthoringError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    let definition: Definition = serde_json::from_slice(source).map_err(AuthoringError::Json)?;
    let rules = validate(&definition)?;
    let limits = definition.budgets;
    let mut expanded = definition.axiom.clone();
    let mut work = 0usize;
    for _ in 0..definition.iterations {
        work = work
            .checked_add(expanded.len())
            .filter(|count| *count <= limits.max_work)
            .ok_or(AuthoringError::Budget("work"))?;
        let mut next = String::new();
        for symbol in expanded.chars() {
            let replacement = rules.get(&symbol).copied();
            let length = replacement.map_or(symbol.len_utf8(), str::len);
            let requested = next
                .len()
                .checked_add(length)
                .filter(|count| *count <= limits.max_symbols)
                .ok_or(AuthoringError::Budget("symbols"))?;
            next.try_reserve(requested - next.len())
                .map_err(|_| AuthoringError::Budget("memory"))?;
            match replacement {
                Some(value) => next.push_str(value),
                None => next.push(symbol),
            }
        }
        expanded = next;
    }
    work = work
        .checked_add(expanded.len())
        .filter(|count| *count <= limits.max_work)
        .ok_or(AuthoringError::Budget("work"))?;
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
    };
    let mut stack = Vec::new();
    let mut segments = Vec::new();
    let mut max_stack_used = 0;
    let yaw = definition.yaw_degrees.to_radians();
    let pitch = definition.pitch_degrees.to_radians();
    for symbol in expanded.chars() {
        match symbol {
            'F' => {
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
                    parent: turtle.parent,
                    start: turtle.position,
                    end,
                    radius: definition.branch_radius,
                });
                turtle.position = end;
                turtle.parent = Some(id);
            }
            '+' => turtle.turn(true, yaw)?,
            '-' => turtle.turn(true, -yaw)?,
            '&' => turtle.turn(false, pitch)?,
            '^' => turtle.turn(false, -pitch)?,
            '[' => {
                if stack.len() >= limits.max_stack_depth {
                    return Err(AuthoringError::Budget("stack"));
                }
                stack
                    .try_reserve(1)
                    .map_err(|_| AuthoringError::Budget("memory"))?;
                stack.push(turtle);
                max_stack_used = max_stack_used.max(stack.len());
            }
            ']' => {
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
    Ok(Structure {
        definition,
        source: source.to_vec(),
        expanded_symbols: expanded.len(),
        max_stack_used,
        work,
        segments,
    })
}

/// Reads at most one byte beyond the input cap, then compiles the file atomically.
pub fn compile_file(path: &Path) -> Result<Structure, AuthoringError> {
    let mut source = Vec::new();
    std::fs::File::open(path)
        .map_err(AuthoringError::Io)?
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut source)
        .map_err(AuthoringError::Io)?;
    compile_json(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
