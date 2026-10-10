//! Bounded WGSL specialization of an already validated Field Graph.

use crate::field_graph_gpu::gpu_nodes;
use world_authoring::field_graph::{Graph, Node};

pub(crate) const SEMANTICS_VERSION: u32 = 1;
const SOURCE: &str = include_str!("../shaders/field_graph.wgsl");
const SAMPLE_BEGIN: &str = "// GENESIS_SAMPLE_BEGIN";
const SAMPLE_END: &str = "// GENESIS_SAMPLE_END";
const MAX_SHADER_BYTES: usize = 32 * 1024;

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
    let nodes = gpu_nodes(graph);
    if nodes.iter().any(|node| {
        node.param.iter().any(|value| !value.is_finite())
            || matches!(node.links[0], 0 | 3 | 5) && node.param[0] <= 0.0
    }) {
        return None;
    }

    fn emit(index: usize, graph: &Graph, params: &[[f32; 4]], seen: &mut [bool], out: &mut String) {
        if seen[index] {
            return;
        }
        let node = &graph.nodes()[index];
        match node {
            Node::Translate { child, .. } | Node::Scale { child, .. } => {
                emit(*child as usize, graph, params, seen, out);
            }
            Node::Union { left, right } | Node::SmoothUnion { left, right, .. } => {
                emit(*left as usize, graph, params, seen, out);
                emit(*right as usize, graph, params, seen, out);
            }
            Node::Sphere(_) | Node::Box(_) => {}
        }
        seen[index] = true;
        let p = params[index];
        let body = match node {
            Node::Sphere(_) => format!("return length(point) - {};", literal(p[0])),
            Node::Box(_) => format!(
                "let q = abs(point) - {}; return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);",
                vector(p)
            ),
            Node::Translate { child, .. } => {
                format!("return gen_eval_{child}(point - {});", vector(p))
            }
            Node::Scale { child, .. } => format!(
                "return gen_eval_{child}(point / {}) * {};",
                literal(p[0]),
                literal(p[0])
            ),
            Node::Union { left, right } => {
                format!("return min(gen_eval_{left}(point), gen_eval_{right}(point));")
            }
            Node::SmoothUnion { left, right, .. } => {
                let width = literal(p[0]);
                format!(
                    "let a = gen_eval_{left}(point); let b = gen_eval_{right}(point); let blend = max({width} - abs(a - b), 0.0) / {width}; return min(a, b) - {width} * blend * blend / 4.0;"
                )
            }
        };
        out.push_str(&format!(
            "fn gen_eval_{index}(point: vec3<f32>) -> f32 {{ {body} }}\n"
        ));
    }

    let params = nodes.iter().map(|node| node.param).collect::<Vec<_>>();
    let mut generated = String::new();
    let mut seen = vec![false; nodes.len()];
    emit(
        graph.root() as usize,
        graph,
        &params,
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
}
