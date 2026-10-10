//! Bounded WGSL specialization of an already validated Field Graph.

use crate::field_graph_gpu::gpu_nodes;
use world_authoring::field_graph::{Graph, Node};

pub(crate) const SEMANTICS_VERSION: u32 = 1;
const SOURCE: &str = include_str!("../shaders/field_graph.wgsl");
const SAMPLE_BEGIN: &str = "// GENESIS_SAMPLE_BEGIN";
const SAMPLE_END: &str = "// GENESIS_SAMPLE_END";
const MAX_SHADER_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct StructuralKey {
    root: u32,
    links: Vec<[u32; 4]>,
}

pub(crate) fn structural_key(graph: &Graph) -> Option<StructuralKey> {
    let nodes = gpu_nodes(graph);
    valid_parameters(&nodes).then(|| StructuralKey {
        root: graph.root(),
        links: nodes.iter().map(|node| node.links).collect(),
    })
}

fn valid_parameters(nodes: &[crate::field_graph_gpu::GpuNode]) -> bool {
    nodes.iter().all(|node| {
        node.param.iter().all(|value| value.is_finite())
            && (!matches!(node.links[0], 0 | 3 | 5) || node.param[0] > 0.0)
    })
}

fn literal(value: f32) -> String {
    format!("{value:e}")
}

fn vector(values: [f32; 4]) -> String {
    format!(
        "vec3<f32>({}, {}, {})",
        literal(values[0]),
        literal(values[1]),
        literal(values[2])
    )
}

/// Returns `None` when f64 parameters cannot retain finite, positive GPU values
/// or the generated shader exceeds the fixed source budget. Callers then interpret.
pub(crate) fn specialized_field_source(graph: &Graph) -> Option<String> {
    field_source(graph, false)
}

/// A graph with the same validated node links can reuse this shader while its
/// current numerical parameters are read from the existing field-node buffer.
pub(crate) fn structural_field_source(graph: &Graph) -> Option<String> {
    field_source(graph, true)
}

/// Emit one bounded program for distinct, validated topologies in key order.
/// A bad local selector uses the reference interpreter instead of hiding geometry.
pub(crate) fn combined_field_source(graphs: &[Graph]) -> Option<String> {
    if graphs.is_empty() || graphs.len() > 4 {
        return None;
    }
    let mut generated = String::new();
    let mut previous = None;
    let mut dispatch = String::new();
    for (slot, graph) in graphs.iter().enumerate() {
        let key = structural_key(graph)?;
        if previous.as_ref().is_some_and(|old| old >= &key) {
            return None;
        }
        previous = Some(key);
        let source = structural_field_source(graph)?;
        let fragment = source.split_once(SAMPLE_BEGIN)?.1.split_once(SAMPLE_END)?.0;
        let functions = fragment.split_once("fn sample(")?.0;
        // shortcut: reuse our generated function text; share the emitter if its format changes.
        generated.push_str(&functions.replace("gen_eval_", &format!("gen_{slot}_eval_")));
        dispatch.push_str(&format!(
            "case {slot}u: {{ return gen_{slot}_eval_{}(point); }}\n",
            graph.root()
        ));
    }
    let interpreter = SOURCE.split_once(SAMPLE_BEGIN)?.1.split_once(SAMPLE_END)?.0;
    generated.push_str(&interpreter.replace("fn sample(", "fn interpreted_sample("));
    generated.push_str(&format!(
        "fn sample(point: vec3<f32>) -> f32 {{ switch field_config.info.w {{ {dispatch} default: {{ return interpreted_sample(point); }} }} }}\n"
    ));
    let (prefix, rest) = SOURCE.split_once(SAMPLE_BEGIN)?;
    let (_, suffix) = rest.split_once(SAMPLE_END)?;
    let source = format!("{prefix}{SAMPLE_BEGIN}\n{generated}{SAMPLE_END}{suffix}");
    (source.len() <= MAX_SHADER_BYTES).then_some(source)
}

fn field_source(graph: &Graph, runtime_parameters: bool) -> Option<String> {
    let nodes = gpu_nodes(graph);
    if !valid_parameters(&nodes) {
        return None;
    }

    fn emit(
        index: usize,
        graph: &Graph,
        params: &[[f32; 4]],
        runtime_parameters: bool,
        seen: &mut [bool],
        out: &mut String,
    ) {
        if seen[index] {
            return;
        }
        let node = &graph.nodes()[index];
        match node {
            Node::Translate { child, .. } | Node::Scale { child, .. } => {
                emit(
                    *child as usize,
                    graph,
                    params,
                    runtime_parameters,
                    seen,
                    out,
                );
            }
            Node::Union { left, right } | Node::SmoothUnion { left, right, .. } => {
                emit(*left as usize, graph, params, runtime_parameters, seen, out);
                emit(
                    *right as usize,
                    graph,
                    params,
                    runtime_parameters,
                    seen,
                    out,
                );
            }
            Node::Sphere(_) | Node::Box(_) => {}
        }
        seen[index] = true;
        let p = params[index];
        let scalar = if runtime_parameters {
            "param.x".to_owned()
        } else {
            literal(p[0])
        };
        let position = if runtime_parameters {
            "param.xyz".to_owned()
        } else {
            vector(p)
        };
        let body = match node {
            Node::Sphere(_) => format!("return length(point) - {scalar};"),
            Node::Box(_) => format!(
                "let q = abs(point) - {position}; return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);"
            ),
            Node::Translate { child, .. } => {
                format!("return gen_eval_{child}(point - {position});")
            }
            Node::Scale { child, .. } => {
                format!("return gen_eval_{child}(point / {scalar}) * {scalar};")
            }
            Node::Union { left, right } => {
                format!("return min(gen_eval_{left}(point), gen_eval_{right}(point));")
            }
            Node::SmoothUnion { left, right, .. } => {
                format!(
                    "let a = gen_eval_{left}(point); let b = gen_eval_{right}(point); let blend = max({scalar} - abs(a - b), 0.0) / {scalar}; return min(a, b) - {scalar} * blend * blend / 4.0;"
                )
            }
        };
        let parameter_load = if runtime_parameters {
            format!(
                "let param = field_nodes[field_config.info.x - {}u + {index}u].param; ",
                graph.root()
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "fn gen_eval_{index}(point: vec3<f32>) -> f32 {{ {parameter_load}{body} }}\n"
        ));
    }

    let params = nodes.iter().map(|node| node.param).collect::<Vec<_>>();
    let mut generated = String::new();
    let mut seen = vec![false; nodes.len()];
    emit(
        graph.root() as usize,
        graph,
        &params,
        runtime_parameters,
        &mut seen,
        &mut generated,
    );
    generated.push_str(&format!(
        "fn sample(point: vec3<f32>) -> f32 {{ return gen_eval_{}(point); }}\n",
        graph.root()
    ));
    let (prefix, rest) = SOURCE
        .split_once(SAMPLE_BEGIN)
        .expect("sample module marker");
    let (_, suffix) = rest.split_once(SAMPLE_END).expect("sample module marker");
    let source = format!("{prefix}{SAMPLE_BEGIN}\n{generated}{SAMPLE_END}{suffix}");
    (source.len() <= MAX_SHADER_BYTES).then_some(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_authoring::field_graph::compile_json;

    #[test]
    fn deterministic_and_reachable_shader() {
        let graph = compile_json(br#"{"format_version":1,"id":"compiler","root":"both","nodes":[{"op":"sphere","id":"ball","radius":1},{"op":"translate","id":"left","child":"ball","offset":[1,0,0]},{"op":"translate","id":"right","child":"ball","offset":[-1,0,0]},{"op":"union","id":"both","left":"left","right":"right"}]}"#).unwrap();
        let source = specialized_field_source(&graph).unwrap();
        assert_eq!(source, specialized_field_source(&graph).unwrap());
        assert!(source.contains("fn gen_eval_0("));
        assert!(source.contains("fn gen_eval_2("));
        assert!(source.contains("fn gen_eval_3("));
        assert_eq!(source.matches("fn gen_eval_0(").count(), 1);
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn structural_shader_reuses_topology_but_not_authored_revision() {
        let first = compile_json(br#"{"format_version":1,"id":"first","root":"u","nodes":[{"op":"sphere","id":"s","radius":0.5},{"op":"translate","id":"t","child":"s","offset":[1,0,0]},{"op":"union","id":"u","left":"s","right":"t"}]}"#).unwrap();
        let second = compile_json(br#"{"format_version":1,"id":"second","root":"u","nodes":[{"op":"sphere","id":"s","radius":0.8},{"op":"translate","id":"t","child":"s","offset":[2,0,0]},{"op":"union","id":"u","left":"s","right":"t"}]}"#).unwrap();
        let other = compile_json(br#"{"format_version":1,"id":"other","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.8}]}"#).unwrap();
        assert_ne!(first.revision(), second.revision());
        assert_eq!(structural_key(&first), structural_key(&second));
        assert_ne!(structural_key(&first), structural_key(&other));
        assert_eq!(
            structural_field_source(&first),
            structural_field_source(&second)
        );
        assert_ne!(
            specialized_field_source(&first),
            specialized_field_source(&second)
        );
        let module =
            naga::front::wgsl::parse_str(&structural_field_source(&first).unwrap()).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn combined_shader_validates_and_rejects_duplicate_topologies() {
        let sphere = compile_json(br#"{"format_version":1,"id":"sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.7}]}"#).unwrap();
        let box_graph = compile_json(br#"{"format_version":1,"id":"box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.7,0.6,0.5]}]}"#).unwrap();
        let mut graphs = vec![sphere.clone(), box_graph];
        graphs.sort_by_key(|graph| structural_key(graph).unwrap());
        let source = combined_field_source(&graphs).unwrap();
        assert!(source.contains("return interpreted_sample(point)"));
        assert_eq!(source, combined_field_source(&graphs).unwrap());
        assert!(combined_field_source(&[sphere.clone(), sphere]).is_none());
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
