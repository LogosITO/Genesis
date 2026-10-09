//! Bounded, blocking mixed-geometry GPU readback for depth-policy research.

use crate::{
    Camera, GpuRenderer, GpuResult, RenderError, Scene,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy, WorldFieldGpuOutcome},
};
use world_state::{EntityId, WorldState};

/// Display source for one pixel. GPU distances remain approximate `f32` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MixedSource {
    /// No supported geometry was hit.
    Miss,
    /// Analytic primitive in the derived snapshot.
    Analytic(u32),
    /// World-owned visual field occurrence.
    Field(EntityId),
    /// A field query was numerically uncertain, even if an analytic hit exists.
    Uncertain,
    /// A field query exhausted its iteration budget.
    Exhausted,
    /// Approximate hit distances were too close for this display policy to order.
    Ambiguous,
}

/// Blocking offscreen result; each pixel has an inspectable source classification.
pub struct MixedFrame {
    /// Tightly packed RGBA8 pixels, top row first.
    pub rgba: Vec<u8>,
    /// One source per pixel in row-major order.
    pub sources: Vec<MixedSource>,
    /// Image dimensions.
    pub size: [u32; 2],
}

/// Bounded display resolution and field tracing policy.
pub struct MixedRenderOptions {
    /// Width and height, each 1..=256 pixels.
    pub size: [u32; 2],
    /// Approximate field tracing settings.
    pub policy: GraphTracePolicy,
}

fn choose(analytic: Option<GpuResult>, field: WorldFieldGpuOutcome) -> MixedSource {
    if analytic.is_some_and(|hit| {
        !hit.distance.is_finite() || hit.distance < 0.0 || hit.normal.iter().any(|n| !n.is_finite())
    }) {
        return MixedSource::Uncertain;
    }
    match field {
        WorldFieldGpuOutcome::Uncertain { .. } => MixedSource::Uncertain,
        WorldFieldGpuOutcome::Exhausted { .. } => MixedSource::Exhausted,
        WorldFieldGpuOutcome::Miss => {
            analytic.map_or(MixedSource::Miss, |hit| MixedSource::Analytic(hit.id))
        }
        WorldFieldGpuOutcome::Hit {
            id,
            distance,
            normal,
        } => {
            if !distance.is_finite() || distance < 0.0 || normal.iter().any(|n| !n.is_finite()) {
                return MixedSource::Uncertain;
            }
            match analytic {
                None => MixedSource::Field(id),
                Some(hit) => {
                    // shortcut: this tie band is a display diagnostic, not a certified f32 error bound.
                    let band = 1e-4 * distance.abs().max(hit.distance.abs()).max(1.0);
                    if (distance - hit.distance).abs() <= band {
                        MixedSource::Ambiguous
                    } else if distance < hit.distance {
                        MixedSource::Field(id)
                    } else {
                        MixedSource::Analytic(hit.id)
                    }
                }
            }
        }
    }
}

fn lit(base: [f32; 3], normal: [f32; 3]) -> [u8; 3] {
    let light = [0.447_213_6, 0.715_541_8, -0.536_656_3];
    let diffuse =
        0.18 + 0.82 * (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).max(0.0);
    base.map(|component| (component * diffuse * 255.0).clamp(0.0, 255.0) as u8)
}

fn color(
    scene: &Scene,
    source: MixedSource,
    analytic: Option<GpuResult>,
    field: WorldFieldGpuOutcome,
    y: u32,
    height: u32,
) -> Result<[u8; 4], RenderError> {
    let rgb =
        match source {
            MixedSource::Miss => {
                let fade = 1.0 - (y as f32 + 0.5) / height as f32;
                [0.035 + 0.03 * fade, 0.055 + 0.04 * fade, 0.09 + 0.05 * fade]
                    .map(|c| (c * 255.0) as u8)
            }
            MixedSource::Analytic(id) => {
                let primitive = scene.primitives().iter().find(|p| p.id == id).ok_or(
                    RenderError::InvalidInput("GPU returned unknown primitive ID"),
                )?;
                let hit = analytic.ok_or(RenderError::InvalidInput("missing analytic hit"))?;
                lit(primitive.color(), hit.normal)
            }
            MixedSource::Field(id) => {
                let WorldFieldGpuOutcome::Hit {
                    id: hit_id, normal, ..
                } = field
                else {
                    return Err(RenderError::InvalidInput("missing field hit"));
                };
                if id != hit_id {
                    return Err(RenderError::InvalidInput("field identity mismatch"));
                }
                lit([0.1, 0.65, 0.8], normal)
            }
            MixedSource::Uncertain => [230, 25, 25],
            MixedSource::Exhausted => [230, 190, 15],
            MixedSource::Ambiguous => [220, 35, 200],
        };
    Ok([rgb[0], rgb[1], rgb[2], 255])
}

/// Compose actual analytic and field GPU query readbacks using the same camera rays.
/// This bounded CPU compositor is a diagnostic reference, not a real-time viewport.
pub fn render_mixed_rgba(
    analytic: &GpuRenderer,
    fields: &GraphGpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world: &WorldState,
    camera: Camera,
    options: MixedRenderOptions,
) -> Result<MixedFrame, RenderError> {
    let MixedRenderOptions { size, policy } = options;
    if size[0] == 0 || size[1] == 0 || size[0] > 256 || size[1] > 256 {
        return Err(RenderError::InvalidInput(
            "mixed diagnostic image outside 1..=256 pixels",
        ));
    }
    let scene = Scene::from_world(world)?;
    let mut rgba = Vec::with_capacity(size[0] as usize * size[1] as usize * 4);
    let mut sources = Vec::with_capacity(size[0] as usize * size[1] as usize);
    for start in (0..size[0] * size[1]).step_by(4096) {
        let end = (start + 4096).min(size[0] * size[1]);
        let rays = (start..end)
            .map(|index| camera.ray(index % size[0], index / size[0], size[0], size[1]))
            .collect::<Result<Vec<_>, _>>()?;
        let analytic_hits = analytic.query(device, queue, &scene, &rays)?;
        let field_hits = fields.query_world_fields(device, queue, world, camera, &rays, policy)?;
        for (offset, (analytic_hit, field_hit)) in
            analytic_hits.into_iter().zip(field_hits).enumerate()
        {
            let source = choose(analytic_hit, field_hit);
            rgba.extend_from_slice(&color(
                &scene,
                source,
                analytic_hit,
                field_hit,
                (start + offset as u32) / size[0],
                size[1],
            )?);
            sources.push(source);
        }
    }
    Ok(MixedFrame {
        rgba,
        sources,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_graph_gpu::GraphGpuIssue;

    fn id() -> EntityId {
        serde_json::from_str("3").unwrap()
    }

    fn analytic(distance: f32) -> Option<GpuResult> {
        Some(GpuResult {
            id: 7,
            distance,
            normal: [0.0, 0.0, -1.0],
        })
    }
    fn field(distance: f32) -> WorldFieldGpuOutcome {
        WorldFieldGpuOutcome::Hit {
            id: id(),
            distance,
            normal: [0.0, 0.0, -1.0],
        }
    }
    #[test]
    fn depth_and_uncertainty_policy_preserves_visible_evidence() {
        assert_eq!(choose(analytic(2.0), field(3.0)), MixedSource::Analytic(7));
        assert_eq!(choose(analytic(3.0), field(2.0)), MixedSource::Field(id()));
        assert_eq!(choose(analytic(2.0), field(2.0)), MixedSource::Ambiguous);
        assert_eq!(
            choose(
                analytic(2.0),
                WorldFieldGpuOutcome::Uncertain {
                    id: id(),
                    issue: GraphGpuIssue::Inside
                }
            ),
            MixedSource::Uncertain
        );
        assert_eq!(
            choose(analytic(2.0), WorldFieldGpuOutcome::Exhausted { id: id() }),
            MixedSource::Exhausted
        );
        assert_eq!(choose(None, WorldFieldGpuOutcome::Miss), MixedSource::Miss);
        assert_eq!(
            choose(analytic(f32::NAN), WorldFieldGpuOutcome::Miss),
            MixedSource::Uncertain
        );
        assert_eq!(choose(None, field(f32::INFINITY)), MixedSource::Uncertain);
    }
}
