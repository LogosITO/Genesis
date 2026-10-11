//! GR-1 research-only occupied-point index over shared mathematical programs.
//! It does not approximate the signed field value or grant collision authority.

use spatial_math::{MathError, Vec3};
use std::{sync::Arc, time::Instant};
use world_authoring::field_graph::{Graph, compile_json};

const ARCH: &[u8] = include_bytes!("../../../examples/field-graph/definitions/gr1-arch.json");
const ORGANIC: &[u8] =
    include_bytes!("../../../examples/field-graph/definitions/gr1-organic-loop.json");

#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn for_instance(graph: &Graph, translation: Vec3) -> Self {
        let local = graph.bounds();
        let offset = [translation.x(), translation.y(), translation.z()];
        let low = [local.min.x(), local.min.y(), local.min.z()];
        let high = [local.max.x(), local.max.y(), local.max.z()];
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for axis in 0..3 {
            min[axis] = (low[axis] + offset[axis]).next_down();
            max[axis] = (high[axis] + offset[axis]).next_up();
        }
        Self { min, max }
    }

    fn union(self, other: Self) -> Self {
        let mut result = self;
        for axis in 0..3 {
            result.min[axis] = result.min[axis].min(other.min[axis]);
            result.max[axis] = result.max[axis].max(other.max[axis]);
        }
        result
    }

    fn contains(self, point: Vec3) -> bool {
        let p = [point.x(), point.y(), point.z()];
        (0..3).all(|axis| p[axis] >= self.min[axis] && p[axis] <= self.max[axis])
    }
}

struct Instance {
    graph: Arc<Graph>,
    translation: Vec3,
    bounds: Bounds,
}

impl Instance {
    fn new(graph: Arc<Graph>, translation: Vec3) -> Self {
        let bounds = Bounds::for_instance(&graph, translation);
        Self {
            graph,
            translation,
            bounds,
        }
    }

    fn occupied(&self, point: Vec3) -> Result<bool, MathError> {
        Ok(self.graph.sample(point.checked_sub(self.translation)?)? <= 0.0)
    }
}

enum Node {
    Leaf {
        bounds: Bounds,
        instance: usize,
    },
    Branch {
        bounds: Bounds,
        left: usize,
        right: usize,
    },
}

impl Node {
    fn bounds(&self) -> Bounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }
}

struct Hierarchy {
    nodes: Vec<Node>,
    root: usize,
}

impl Hierarchy {
    fn build(instances: &[Instance]) -> Self {
        assert!(!instances.is_empty() && instances.len() <= 128);
        let mut nodes = Vec::with_capacity(instances.len() * 2 - 1);
        let mut indices: Vec<usize> = (0..instances.len()).collect();
        let root = Self::build_range(instances, &mut indices, &mut nodes, 0);
        Self { nodes, root }
    }

    fn build_range(
        instances: &[Instance],
        indices: &mut [usize],
        nodes: &mut Vec<Node>,
        axis: usize,
    ) -> usize {
        if indices.len() == 1 {
            let instance = indices[0];
            let id = nodes.len();
            nodes.push(Node::Leaf {
                bounds: instances[instance].bounds,
                instance,
            });
            return id;
        }
        indices.sort_by(|a, b| {
            let center = |index: usize| {
                let bounds = instances[index].bounds;
                (bounds.min[axis] + bounds.max[axis]) * 0.5
            };
            center(*a).total_cmp(&center(*b)).then_with(|| a.cmp(b))
        });
        let (left, right) = indices.split_at_mut(indices.len() / 2);
        let left = Self::build_range(instances, left, nodes, (axis + 1) % 3);
        let right = Self::build_range(instances, right, nodes, (axis + 1) % 3);
        let bounds = nodes[left].bounds().union(nodes[right].bounds());
        let id = nodes.len();
        nodes.push(Node::Branch {
            bounds,
            left,
            right,
        });
        id
    }

    fn occupied(&self, instances: &[Instance], point: Vec3) -> Result<(bool, usize), MathError> {
        let mut stack = vec![self.root];
        let mut samples = 0;
        while let Some(index) = stack.pop() {
            match self.nodes[index] {
                Node::Leaf { bounds, instance } if bounds.contains(point) => {
                    samples += 1;
                    if instances[instance].occupied(point)? {
                        return Ok((true, samples));
                    }
                }
                Node::Branch {
                    bounds,
                    left,
                    right,
                } if bounds.contains(point) => {
                    stack.push(right);
                    stack.push(left);
                }
                _ => {}
            }
        }
        Ok((false, samples))
    }
}

fn flat_occupied(instances: &[Instance], point: Vec3) -> Result<(bool, usize), MathError> {
    for (index, instance) in instances.iter().enumerate() {
        if instance.occupied(point)? {
            return Ok((true, index + 1));
        }
    }
    Ok((false, instances.len()))
}

fn instances(source: &[u8], count: usize, overlapping: bool) -> Vec<Instance> {
    let graph = Arc::new(compile_json(source).unwrap());
    (0..count)
        .map(|index| {
            let (x, y) = if overlapping {
                (0.0, 0.0)
            } else {
                ((index % 16) as f64 * 3.0, (index / 16) as f64 * 3.0)
            };
            Instance::new(Arc::clone(&graph), Vec3::new(x, y, 0.0).unwrap())
        })
        .collect()
}

fn points() -> Vec<Vec3> {
    (0..64)
        .flat_map(|y| {
            (0..64)
                .map(move |x| Vec3::new(x as f64 * 0.8 - 1.0, y as f64 * 0.4 - 1.0, 0.0).unwrap())
        })
        .collect()
}

fn crowded_points() -> Vec<Vec3> {
    (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| {
                Vec3::new(
                    x as f64 * 0.6 / 63.0 - 0.3,
                    y as f64 * 0.6 / 63.0 - 0.3,
                    0.0,
                )
                .unwrap()
            })
        })
        .collect()
}

#[test]
fn hierarchy_matches_flat_occupied_set_for_supported_points() {
    for source in [ARCH, ORGANIC] {
        for (count, overlapping) in [
            (1, false),
            (8, false),
            (32, false),
            (128, false),
            (32, true),
        ] {
            let instances = instances(source, count, overlapping);
            let hierarchy = Hierarchy::build(&instances);
            assert_eq!(hierarchy.nodes.len(), count * 2 - 1);
            for point in if overlapping {
                crowded_points()
            } else {
                points()
            } {
                let flat = flat_occupied(&instances, point).unwrap().0;
                let indexed = hierarchy.occupied(&instances, point).unwrap().0;
                assert_eq!(
                    flat, indexed,
                    "count={count} overlapping={overlapping} point={point:?}"
                );
            }
        }
    }
}

#[test]
fn boundary_and_rebuilt_instance_match_analytic_arch() {
    let graph = Arc::new(compile_json(ARCH).unwrap());
    let mut instances = vec![Instance::new(
        Arc::clone(&graph),
        Vec3::new(0.0, 0.0, 0.0).unwrap(),
    )];
    for (point, expected) in [
        ([-0.7, 0.0, 0.0], true),
        ([-0.82, 0.0, 0.0], true),
        ([-1.0, 0.0, 0.0], false),
        ([0.0, 0.8, 0.0], true),
        ([0.0, 0.0, 0.0], false),
    ] {
        let point = Vec3::new(point[0], point[1], point[2]).unwrap();
        assert_eq!(flat_occupied(&instances, point).unwrap().0, expected);
        assert_eq!(
            Hierarchy::build(&instances)
                .occupied(&instances, point)
                .unwrap()
                .0,
            expected
        );
    }

    instances[0] = Instance::new(graph, Vec3::new(20.0, 0.0, 0.0).unwrap());
    let rebuilt = Hierarchy::build(&instances);
    for (point, expected) in [([-0.7, 0.0, 0.0], false), ([19.3, 0.0, 0.0], true)] {
        let point = Vec3::new(point[0], point[1], point[2]).unwrap();
        assert_eq!(flat_occupied(&instances, point).unwrap().0, expected);
        assert_eq!(rebuilt.occupied(&instances, point).unwrap().0, expected);
    }
}

#[test]
#[ignore = "research benchmark; run in release profile and record hardware"]
fn benchmark_flat_and_hierarchical_occupancy() {
    for (name, source) in [("arch", ARCH), ("organic", ORGANIC)] {
        for (count, overlapping) in [
            (1, false),
            (8, false),
            (32, false),
            (128, false),
            (32, true),
        ] {
            let queries = if overlapping {
                crowded_points()
            } else {
                points()
            };
            let started = Instant::now();
            let instances = instances(source, count, overlapping);
            let hierarchy = Hierarchy::build(&instances);
            let build_ms = started.elapsed().as_secs_f64() * 1000.0;
            let mut results = Vec::new();
            for method in ["flat", "hierarchy"] {
                let mut samples = 0;
                let mut occupied = 0;
                let mut timings = Vec::new();
                for _ in 0..7 {
                    let started = Instant::now();
                    for &point in &queries {
                        let (inside, work) = if method == "flat" {
                            flat_occupied(&instances, point).unwrap()
                        } else {
                            hierarchy.occupied(&instances, point).unwrap()
                        };
                        samples += work;
                        occupied += usize::from(inside);
                    }
                    timings.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                timings.sort_by(f64::total_cmp);
                results.push(serde_json::json!({"method":method,"median_ms":timings[3],"samples":samples,"occupied":occupied}));
            }
            assert_eq!(results[0]["occupied"], results[1]["occupied"]);
            println!(
                "{}",
                serde_json::json!({
                    "specimen":name,"instances":count,"overlapping":overlapping,
                    "graph_revision":instances[0].graph.revision().hex(),
                    "graph_nodes":instances[0].graph.nodes().len(),"graph_visits":instances[0].graph.visits(),
                    "queries":queries.len(),"repeats":7,"build_ms":build_ms,
                    "hierarchy_nodes":hierarchy.nodes.len(),
                    "hierarchy_node_bytes":hierarchy.nodes.len()*std::mem::size_of::<Node>(),
                    "instance_record_bytes":instances.len()*std::mem::size_of::<Instance>(),
                    "results":results
                })
            );
        }
    }
}
