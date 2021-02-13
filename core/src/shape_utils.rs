use crate::bounding_box::BoundingBox;
use fnv::FnvHashMap;
use smallvec::SmallVec;
use std::num::NonZeroU32;
use swf::{CharacterId, FillStyle, LineStyle, Matrix, Shape, ShapeRecord, Twips};

pub fn calculate_shape_bounds(shape_records: &[swf::ShapeRecord]) -> BoundingBox {
    let mut bounds = BoundingBox::default();
    let mut x = Twips::new(0);
    let mut y = Twips::new(0);
    for record in shape_records {
        match record {
            swf::ShapeRecord::StyleChange(style_change) => {
                if let Some((move_x, move_y)) = style_change.move_to {
                    x = move_x;
                    y = move_y;
                    bounds.union(&BoundingBox {
                        x_min: x,
                        x_max: x,
                        y_min: y,
                        y_max: y,
                    });
                }
            }
            swf::ShapeRecord::StraightEdge { delta_x, delta_y } => {
                x += *delta_x;
                y += *delta_y;
                bounds.union(&BoundingBox {
                    x_min: x,
                    x_max: x,
                    y_min: y,
                    y_max: y,
                });
            }
            swf::ShapeRecord::CurvedEdge {
                control_delta_x,
                control_delta_y,
                anchor_delta_x,
                anchor_delta_y,
            } => {
                x += *control_delta_x;
                y += *control_delta_y;
                bounds.union(&BoundingBox {
                    x_min: x,
                    x_max: x,
                    y_min: y,
                    y_max: y,
                });
                x += *anchor_delta_x;
                y += *anchor_delta_y;
                bounds.union(&BoundingBox {
                    x_min: x,
                    x_max: x,
                    y_min: y,
                    y_max: y,
                });
            }
        }
    }
    bounds
}

/// `DrawPath` represents a solid fill or a stroke.
/// Fills are always closed paths, while strokes may be open or closed.
/// Closed paths will have the first point equal to the last point.
#[derive(Debug, PartialEq, Clone)]
pub enum DrawPath<'a> {
    Stroke {
        style: &'a LineStyle,
        is_closed: bool,
        commands: Vec<DrawCommand>,
    },
    Fill {
        style: &'a FillStyle,
        commands: Vec<DrawCommand>,
    },
}

/// `DistilledShape` represents a ready-to-be-consumed collection of paths (both fills and strokes)
/// that has been converted down from another source (such as SWF's `swf::Shape` format).
#[derive(Debug, PartialEq, Clone)]
pub struct DistilledShape<'a> {
    pub paths: Vec<DrawPath<'a>>,
    pub shape_bounds: BoundingBox,
    pub edge_bounds: BoundingBox,
    pub id: CharacterId,
}

impl<'a> From<&'a swf::Shape> for DistilledShape<'a> {
    fn from(shape: &'a Shape) -> Self {
        Self {
            paths: ShapeConverter::from_shape(shape).into_commands(),
            shape_bounds: (&shape.shape_bounds).into(),
            edge_bounds: (&shape.edge_bounds).into(),
            id: shape.id,
        }
    }
}

/// `DrawCommands` trace the outline of a path.
/// Fills follow the even-odd fill rule, with opposite winding for holes.
#[derive(Debug, PartialEq, Clone)]
pub enum DrawCommand {
    MoveTo {
        x: Twips,
        y: Twips,
    },
    LineTo {
        x: Twips,
        y: Twips,
    },
    CurveTo {
        x1: Twips,
        y1: Twips,
        x2: Twips,
        y2: Twips,
    },
}

impl DrawCommand {
    pub fn end_point(&self) -> (Twips, Twips) {
        match self {
            DrawCommand::MoveTo { x, y } => (*x, *y),
            DrawCommand::LineTo { x, y } => (*x, *y),
            DrawCommand::CurveTo { x2, y2, .. } => (*x2, *y2),
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct Point {
    x: Twips,
    y: Twips,
    is_bezier_control: bool,
}

/// A path segment is a series of edges linked togerther.
/// Fill paths are directed, because the winding determines the fill-rule.
/// Stroke paths are undirected.
#[derive(Debug)]
struct PathSegment {
    pub points: Vec<Point>,
}

impl PathSegment {
    fn new(start: (Twips, Twips)) -> Self {
        Self {
            points: vec![Point {
                x: start.0,
                y: start.1,
                is_bezier_control: false,
            }],
        }
    }

    /// Flips the direction of the path segment.
    /// Flash fill paths are dual-sided, with fill style 1 indicating the positive side
    /// and fill style 0 indicating the negative. We have to flip fill style 0 paths
    /// in order to link them to fill style 1 paths.
    fn flip(&mut self) {
        self.points.reverse();
    }

    /// Adds an edge to the end of the path segment.
    fn add_point(&mut self, point: Point) {
        self.points.push(point);
    }

    fn is_empty(&self) -> bool {
        self.points.len() <= 1
    }

    fn start(&self) -> (Twips, Twips) {
        let pt = &self.points.first().unwrap();
        (pt.x, pt.y)
    }

    fn end(&self) -> (Twips, Twips) {
        let pt = &self.points.last().unwrap();
        (pt.x, pt.y)
    }

    fn is_closed(&self) -> bool {
        self.start() == self.end()
    }

    /// Attempts to merge another path segment.
    /// One path's start must meet the other path's end.
    /// Returns true if the merge is successful.
    fn try_merge(&mut self, other: &mut PathSegment, directed: bool) -> bool {
        // Note that the merge point will be duplicated, so we want to slice it off one end. [1..]
        if other.end() == self.start() {
            std::mem::swap(&mut self.points, &mut other.points);
            self.points.extend_from_slice(&other.points[1..]);
            true
        } else if self.end() == other.start() {
            self.points.extend_from_slice(&other.points[1..]);
            true
        } else if !directed && self.end() == other.end() {
            other.flip();
            self.points.extend_from_slice(&other.points[1..]);
            true
        } else if !directed && self.start() == other.start() {
            other.flip();
            std::mem::swap(&mut self.points, &mut other.points);
            self.points.extend_from_slice(&other.points[1..]);
            true
        } else {
            false
        }
    }

    fn into_draw_commands(self) -> impl Iterator<Item = DrawCommand> {
        assert!(self.points.len() > 1);
        let mut i = self.points.into_iter();
        let first = i.next().unwrap();
        std::iter::once(DrawCommand::MoveTo {
            x: first.x,
            y: first.y,
        })
        .chain(std::iter::from_fn(move || match i.next() {
            Some(Point {
                is_bezier_control: false,
                x,
                y,
            }) => Some(DrawCommand::LineTo { x, y }),
            Some(Point {
                is_bezier_control: true,
                x,
                y,
            }) => {
                let end = i.next().expect("Bezier without endpoint");
                Some(DrawCommand::CurveTo {
                    x1: x,
                    y1: y,
                    x2: end.x,
                    y2: end.y,
                })
            }
            None => None,
        }))
    }
}

/// The internal path structure used by ShapeConverter.
///
/// Each path is uniquely identified by its fill/stroke style. But Flash gives
/// the path edges as an "edge soup" -- they can arrive in an arbitrary order.
/// We have to link the edges together for each path. This structure contains
/// a list of path segment, and each time a path segment is added, it will try
/// to merge it with an existing segment.
#[derive(Debug)]
struct PendingPath {
    /// The list of path segments for this fill/stroke.
    /// For fills, this should turn into a list of closed paths when the shape is complete.
    /// Strokes may or may not be closed.
    segments: Vec<PathSegment>,
}

impl PendingPath {
    fn new() -> Self {
        Self { segments: vec![] }
    }

    fn merge_path(&mut self, mut new_segment: PathSegment, directed: bool) {
        if !new_segment.is_empty() {
            if let Some(i) = self
                .segments
                .iter_mut()
                .position(|segment| segment.try_merge(&mut new_segment, directed))
            {
                new_segment = self.segments.swap_remove(i);
                self.merge_path(new_segment, directed);
            } else {
                // Couldn't merge the segment any further to an existing segment. Add it to list.
                self.segments.push(new_segment);
            }
        }
    }

    fn into_draw_commands(self) -> impl Iterator<Item = DrawCommand> {
        self.segments
            .into_iter()
            .map(PathSegment::into_draw_commands)
            .flatten()
    }
}

/// `PendingPathMap` maps from style IDs to the path associated with that style.
/// Each path is uniquely identified by its style ID (until the style list changes).
/// Style IDs tend to be sequential, so we just use a `Vec`.
#[derive(Debug)]
pub struct PendingPathMap(FnvHashMap<NonZeroU32, PendingPath>);

impl PendingPathMap {
    fn new() -> Self {
        Self(FnvHashMap::default())
    }

    fn merge_path(&mut self, path: ActivePath, directed: bool) {
        let pending_path = self.0.entry(path.style_id).or_insert_with(PendingPath::new);
        pending_path.merge_path(path.segment, directed);
    }
}

#[derive(Debug)]
pub struct ActivePath {
    style_id: NonZeroU32,
    segment: PathSegment,
}

impl ActivePath {
    fn new(style_id: NonZeroU32, start: (Twips, Twips)) -> Self {
        Self {
            style_id,
            segment: PathSegment::new(start),
        }
    }

    fn add_point(&mut self, point: Point) {
        self.segment.add_point(point)
    }

    fn flip(&mut self) {
        self.segment.flip()
    }
}

pub struct ShapeConverter<'a> {
    // SWF shape commands.
    iter: std::slice::Iter<'a, swf::ShapeRecord>,

    // Pen position.
    x: Twips,
    y: Twips,

    // Fill styles and line styles.
    // These change from StyleChangeRecords, and a flush occurs when these change.
    fill_styles: &'a [swf::FillStyle],
    line_styles: &'a [swf::LineStyle],

    fill_style0: Option<ActivePath>,
    fill_style1: Option<ActivePath>,
    line_style: Option<ActivePath>,

    // Paths. These get flushed when the shape is complete
    // and for each new layer.
    fills: PendingPathMap,
    strokes: PendingPathMap,

    // Output.
    commands: Vec<DrawPath<'a>>,
}

impl<'a> ShapeConverter<'a> {
    const DEFAULT_CAPACITY: usize = 512;

    fn from_shape(shape: &'a swf::Shape) -> Self {
        ShapeConverter {
            iter: shape.shape.iter(),

            x: Twips::new(0),
            y: Twips::new(0),

            fill_styles: &shape.styles.fill_styles,
            line_styles: &shape.styles.line_styles,

            fill_style0: None,
            fill_style1: None,
            line_style: None,

            fills: PendingPathMap::new(),
            strokes: PendingPathMap::new(),

            commands: Vec::with_capacity(Self::DEFAULT_CAPACITY),
        }
    }

    fn into_commands(mut self) -> Vec<DrawPath<'a>> {
        // as u32 is okay because SWF has a max of 65536 fills (TODO: should be u16?)
        let mut num_fill_styles = self.fill_styles.len() as u32;
        let mut num_line_styles = self.line_styles.len() as u32;
        while let Some(record) = self.iter.next() {
            match record {
                ShapeRecord::StyleChange(style_change) => {
                    if let Some((x, y)) = style_change.move_to {
                        self.x = x;
                        self.y = y;
                        // We've lifted the pen, so we're starting a new path.
                        // Flush the previous path.
                        self.flush_paths();
                    }

                    if let Some(ref styles) = style_change.new_styles {
                        // A new style list is also used to indicate a new drawing layer.
                        self.flush_layer();
                        self.fill_styles = &styles.fill_styles[..];
                        self.line_styles = &styles.line_styles[..];
                        num_fill_styles = self.fill_styles.len() as u32;
                        num_line_styles = self.line_styles.len() as u32;
                    }

                    if let Some(fs) = style_change.fill_style_1 {
                        if let Some(path) = self.fill_style1.take() {
                            self.fills.merge_path(path, true);
                        }

                        // <= because fill ID 0 (no fill) is not included in fill style array
                        self.fill_style1 = if fs <= num_fill_styles {
                            NonZeroU32::new(fs).map(|id| ActivePath::new(id, (self.x, self.y)))
                        } else {
                            None
                        }
                    }

                    if let Some(fs) = style_change.fill_style_0 {
                        if let Some(mut path) = self.fill_style0.take() {
                            if !path.segment.is_empty() {
                                path.flip();
                                self.fills.merge_path(path, true);
                            }
                        }

                        self.fill_style0 = if fs <= num_fill_styles {
                            NonZeroU32::new(fs).map(|id| ActivePath::new(id, (self.x, self.y)))
                        } else {
                            None
                        }
                    }

                    if let Some(ls) = style_change.line_style {
                        if let Some(path) = self.line_style.take() {
                            self.strokes.merge_path(path, false);
                        }

                        self.line_style = if ls <= num_line_styles {
                            NonZeroU32::new(ls).map(|id| ActivePath::new(id, (self.x, self.y)))
                        } else {
                            None
                        }
                    }
                }

                ShapeRecord::StraightEdge { delta_x, delta_y } => {
                    self.x += *delta_x;
                    self.y += *delta_y;

                    self.visit_point(Point {
                        x: self.x,
                        y: self.y,
                        is_bezier_control: false,
                    });
                }

                ShapeRecord::CurvedEdge {
                    control_delta_x,
                    control_delta_y,
                    anchor_delta_x,
                    anchor_delta_y,
                } => {
                    let x1 = self.x + *control_delta_x;
                    let y1 = self.y + *control_delta_y;

                    self.visit_point(Point {
                        x: x1,
                        y: y1,
                        is_bezier_control: true,
                    });

                    let x2 = x1 + *anchor_delta_x;
                    let y2 = y1 + *anchor_delta_y;

                    self.visit_point(Point {
                        x: x2,
                        y: y2,
                        is_bezier_control: false,
                    });

                    self.x = x2;
                    self.y = y2;
                }
            }
        }

        // Flush any open paths.
        self.flush_layer();
        self.commands
    }

    /// Adds a point to the current path for the active fills/strokes.
    fn visit_point(&mut self, point: Point) {
        if let Some(path) = &mut self.fill_style0 {
            path.add_point(point)
        }

        if let Some(path) = &mut self.fill_style1 {
            path.add_point(point)
        }

        if let Some(path) = &mut self.line_style {
            path.add_point(point)
        }
    }

    /// When the pen jumps to a new position, we reset the active path.
    fn flush_paths(&mut self) {
        // Move the current paths to the active list.
        if let Some(path) = self.fill_style1.take() {
            self.fill_style1 = Some(ActivePath::new(path.style_id, (self.x, self.y)));
            self.fills.merge_path(path, true);
        }

        if let Some(mut path) = self.fill_style0.take() {
            self.fill_style0 = Some(ActivePath::new(path.style_id, (self.x, self.y)));
            if !path.segment.is_empty() {
                path.flip();
                self.fills.merge_path(path, true);
            }
        }

        if let Some(path) = self.line_style.take() {
            self.line_style = Some(ActivePath::new(path.style_id, (self.x, self.y)));
            self.strokes.merge_path(path, false);
        }
    }

    /// When a new layer starts, all paths are flushed and turned into drawing commands.
    fn flush_layer(&mut self) {
        self.flush_paths();
        self.fill_style0 = None;
        self.fill_style1 = None;
        self.line_style = None;

        //let fills = std::mem::replace(&mut self.fills.0, FnvHashMap::default());
        //let strokes = std::mem::replace(&mut self.strokes.0, FnvHashMap::default());

        // Draw fills, and then strokes.
        for (style_id, path) in self.fills.0.drain() {
            // These invariants are checked above (any invalid/empty fill ID should not have been added).
            assert!(style_id.get() > 0 && style_id.get() as usize <= self.fill_styles.len());
            let style = unsafe { self.fill_styles.get_unchecked(style_id.get() as usize - 1) };
            self.commands.push(DrawPath::Fill {
                style,
                commands: path.into_draw_commands().collect(),
            });
        }

        // Strokes are drawn last because they always appear on top of fills in the same layer.
        // Because path segments can either be open or closed, we convert each stroke segment into
        // a separate draw command.
        // TODO(Herschel): Open strokes could be grouped together into a single path.
        for (style_id, path) in self.strokes.0.drain() {
            assert!(style_id.get() > 0 && style_id.get() as usize <= self.line_styles.len());
            let style = unsafe { self.line_styles.get_unchecked(style_id.get() as usize - 1) };
            for segment in path.segments {
                self.commands.push(DrawPath::Stroke {
                    style,
                    is_closed: segment.is_closed(),
                    commands: segment.into_draw_commands().collect(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILL_STYLES: [FillStyle; 1] = [FillStyle::Color(swf::Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    })];

    const LINE_STYLES: [LineStyle; 0] = [];

    /// Convenience method to quickly make a shape,
    fn build_shape(records: Vec<ShapeRecord>) -> swf::Shape {
        let bounds = calculate_shape_bounds(&records[..]);
        swf::Shape {
            version: 2,
            id: 1,
            shape_bounds: bounds.clone(),
            edge_bounds: bounds,
            has_fill_winding_rule: false,
            has_non_scaling_strokes: false,
            has_scaling_strokes: true,
            styles: swf::ShapeStyles {
                fill_styles: FILL_STYLES.to_vec(),
                line_styles: LINE_STYLES.to_vec(),
            },
            shape: records,
        }
    }

    /// A simple solid square.
    #[test]
    fn basic_shape() {
        let shape = build_shape(vec![
            ShapeRecord::StyleChange(swf::StyleChangeData {
                move_to: Some((Twips::from_pixels(100.0), Twips::from_pixels(100.0))),
                fill_style_0: None,
                fill_style_1: Some(1),
                line_style: None,
                new_styles: None,
            }),
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(100.0),
                delta_y: Twips::from_pixels(0.0),
            },
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(0.0),
                delta_y: Twips::from_pixels(100.0),
            },
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(-100.0),
                delta_y: Twips::from_pixels(0.0),
            },
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(0.0),
                delta_y: Twips::from_pixels(-100.0),
            },
        ]);
        let commands = ShapeConverter::from_shape(&shape).into_commands();
        let expected = vec![DrawPath::Fill {
            style: &FILL_STYLES[0],
            commands: vec![
                DrawCommand::MoveTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(100.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(200.0),
                    y: Twips::from_pixels(100.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(200.0),
                    y: Twips::from_pixels(200.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(200.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(100.0),
                },
            ],
        }];
        assert_eq!(commands, expected);
    }

    /// A solid square with one edge flipped (fillstyle0 instead of fillstyle1).
    #[test]
    fn flipped_edges() {
        let shape = build_shape(vec![
            ShapeRecord::StyleChange(swf::StyleChangeData {
                move_to: Some((Twips::from_pixels(100.0), Twips::from_pixels(100.0))),
                fill_style_0: None,
                fill_style_1: Some(1),
                line_style: None,
                new_styles: None,
            }),
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(100.0),
                delta_y: Twips::from_pixels(0.0),
            },
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(0.0),
                delta_y: Twips::from_pixels(100.0),
            },
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(-100.0),
                delta_y: Twips::from_pixels(0.0),
            },
            ShapeRecord::StyleChange(swf::StyleChangeData {
                move_to: Some((Twips::from_pixels(100.0), Twips::from_pixels(100.0))),
                fill_style_0: Some(1),
                fill_style_1: Some(0),
                line_style: None,
                new_styles: None,
            }),
            ShapeRecord::StraightEdge {
                delta_x: Twips::from_pixels(0.0),
                delta_y: Twips::from_pixels(100.0),
            },
        ]);
        let commands = ShapeConverter::from_shape(&shape).into_commands();
        let expected = vec![DrawPath::Fill {
            style: &FILL_STYLES[0],
            commands: vec![
                DrawCommand::MoveTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(200.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(100.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(200.0),
                    y: Twips::from_pixels(100.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(200.0),
                    y: Twips::from_pixels(200.0),
                },
                DrawCommand::LineTo {
                    x: Twips::from_pixels(100.0),
                    y: Twips::from_pixels(200.0),
                },
            ],
        }];
        assert_eq!(commands, expected);
    }
}

/* SHAPEFLAG HITTEST (point-in-contour)
 *
 * To determine whether a point is inside a shape, we shoot a ray on the +x axis and calculate a winding number based
 * on the edges that intersect with the ray.
 *
 * For each edge:
 *  if the edge cross the ray downward (+y), we add 1 to the winding number.
 *  if the edge cross the ray upward (-y), we add -1 to the winding number.
 *
 * We must also handle intersection with edge endpoints consistently to avoid double counting:
 *  the initial point of an edge is considered for upwards rays.
 *  the final point of an edge is considered for downward rays.
 *
 * For SWF shapes, edges with fillstyle1 use clockwise winding, and edges with fillstyle0 use CCW winding (flip them).
 * We ignore any edges with fills on both sides (interior edges).
 *
 * If the final winding number is odd, then the point is inside the shape (for default even-odd winding).
 *
 * For strokes, we calculate the distance to the line segment or curve and compare it to the stroke width.
 * Note that Flash renders with a minimum stroke width of 1px (20 twips) that we must account for.
 * TODO: We currently don't consider non-round endcaps or joins, or stroke scaling flags.
 */

/// Test whether the given point in object space is contained within the contour of the given shape.
/// local_matrix is used to calculate the proper stroke widths.
pub fn shape_hit_test(
    shape: &swf::Shape,
    (point_x, point_y): (Twips, Twips),
    local_matrix: &Matrix,
) -> bool {
    // Transform point to local space.
    let mut x = Twips::new(0);
    let mut y = Twips::new(0);
    let mut winding = 0;

    let mut has_fill_style0: bool = false;
    let mut has_fill_style1: bool = false;

    let min_width = f64::from(stroke_minimum_width(local_matrix));
    let mut stroke_width = None;
    let mut line_styles = &shape.styles.line_styles;

    for record in &shape.shape {
        match record {
            swf::ShapeRecord::StyleChange(style_change) => {
                // New styles indicates a new layer;
                // Check if the point is within the current layer, then reset winding.
                if let Some(new_styles) = &style_change.new_styles {
                    if winding & 0b1 != 0 {
                        return true;
                    }
                    line_styles = &new_styles.line_styles;
                    winding = 0;
                }

                if let Some((move_x, move_y)) = style_change.move_to {
                    x = move_x;
                    y = move_y;
                }

                if let Some(i) = style_change.fill_style_0 {
                    has_fill_style0 = i > 0;
                }
                if let Some(i) = style_change.fill_style_1 {
                    has_fill_style1 = i > 0;
                }
                if let Some(i) = style_change.line_style {
                    stroke_width = if i > 0 {
                        // Flash renders strokes with a 1px minimum width.
                        if let Some(line_style) = line_styles.get(i as usize - 1) {
                            let width = line_style.width.get() as f64;
                            let scaled_width = 0.5 * width.max(min_width);
                            Some((scaled_width, scaled_width * scaled_width))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                }
            }
            swf::ShapeRecord::StraightEdge { delta_x, delta_y } => {
                let x1 = x + *delta_x;
                let y1 = y + *delta_y;
                // If this edge has a fill style on only one-side, check for a crossing.
                if has_fill_style1 {
                    if !has_fill_style0 {
                        winding += winding_number_line((point_x, point_y), (x, y), (x1, y1));
                    }
                } else if has_fill_style0 {
                    winding += winding_number_line((point_x, point_y), (x1, y1), (x, y));
                }

                if let Some(width) = stroke_width {
                    if hit_test_stroke((point_x, point_y), (x, y), (x1, y1), width) {
                        return true;
                    }
                }
                x = x1;
                y = y1;
            }
            swf::ShapeRecord::CurvedEdge {
                control_delta_x,
                control_delta_y,
                anchor_delta_x,
                anchor_delta_y,
            } => {
                let x1 = x + *control_delta_x;
                let y1 = y + *control_delta_y;

                let x2 = x1 + *anchor_delta_x;
                let y2 = y1 + *anchor_delta_y;

                // If this edge has a fill style on only one-side, check for a crossing.
                if has_fill_style1 {
                    if !has_fill_style0 {
                        winding +=
                            winding_number_curve((point_x, point_y), (x, y), (x1, y1), (x2, y2));
                    }
                } else if has_fill_style0 {
                    winding += winding_number_curve((point_x, point_y), (x2, y2), (x1, y1), (x, y));
                }

                if let Some(width) = stroke_width {
                    if hit_test_stroke_curve((point_x, point_y), (x, y), (x1, y1), (x2, y2), width)
                    {
                        return true;
                    }
                }

                x = x2;
                y = y2;
            }
        }
    }
    winding & 0b1 != 0
}

/// Test whether the given point is contained with in the paths specified by the draw commands.
pub fn draw_command_fill_hit_test(
    commands: &[DrawCommand],
    (point_x, point_y): (Twips, Twips),
) -> bool {
    let mut x = Twips::new(0);
    let mut y = Twips::new(0);
    let mut winding = 0;

    // Draw command only contains a single fill, so don't have to worry about fill styles.
    for command in commands {
        match *command {
            DrawCommand::MoveTo { x: x1, y: y1 } => {
                x = x1;
                y = y1;
            }
            DrawCommand::LineTo { x: x1, y: y1 } => {
                winding += winding_number_line((point_x, point_y), (x, y), (x1, y1));
                x = x1;
                y = y1;
            }
            DrawCommand::CurveTo { x1, y1, x2, y2 } => {
                winding += winding_number_curve((point_x, point_y), (x, y), (x1, y1), (x2, y2));
                x = x2;
                y = y2;
            }
        }
    }
    winding & 0b1 != 0
}

/// Test whether the given point is contained with in the strokes specified by the draw commands.
/// local_matrix is used to calculate the minimum stroke width.
pub fn draw_command_stroke_hit_test(
    commands: &[DrawCommand],
    stroke_width: Twips,
    (point_x, point_y): (Twips, Twips),
    local_matrix: &Matrix,
) -> bool {
    let stroke_min_width = f64::from(stroke_minimum_width(local_matrix));
    let stroke_width = 0.5 * f64::max(stroke_width.get().into(), stroke_min_width);
    let stroke_widths = (stroke_width, stroke_width * stroke_width);
    let mut x = Twips::default();
    let mut y = Twips::default();
    for command in commands {
        match *command {
            DrawCommand::MoveTo { x: x1, y: y1 } => {
                x = x1;
                y = y1;
            }
            DrawCommand::LineTo { x: x1, y: y1 } => {
                if hit_test_stroke((point_x, point_y), (x, y), (x1, y1), stroke_widths) {
                    return true;
                }
                x = x1;
                y = y1;
            }
            DrawCommand::CurveTo { x1, y1, x2, y2 } => {
                if hit_test_stroke_curve(
                    (point_x, point_y),
                    (x, y),
                    (x1, y1),
                    (x2, y2),
                    stroke_widths,
                ) {
                    return true;
                }
                x = x2;
                y = y2;
            }
        }
    }

    false
}

/// Given a matrix, calculates the scale for stroke widths.
/// TODO: Verify the actual behavior; I think it's more like the average between scaleX and scaleY.
/// Does not yet support vertical/horizontal stroke scaling flags.
/// This might be better to add as a method to Matrix.
fn stroke_minimum_width(matrix: &Matrix) -> f32 {
    let sx = (matrix.a * matrix.a + matrix.b * matrix.b).sqrt();
    let sy = (matrix.c * matrix.c + matrix.d * matrix.d).sqrt();
    let scale = sx.max(sy);
    20.0 * scale
}

/// Returns whether the given point is inside the stroked line segment.
/// `width_sq` should be the squared width of the stroke.
fn hit_test_stroke(
    (point_x, point_y): (Twips, Twips),
    (x0, y0): (Twips, Twips),
    (x1, y1): (Twips, Twips),
    (stroke_width, stroke_width_sq): (f64, f64),
) -> bool {
    let px = point_x.get() as f64;
    let py = point_y.get() as f64;
    let x0 = x0.get() as f64;
    let y0 = y0.get() as f64;
    let x1 = x1.get() as f64;
    let y1 = y1.get() as f64;

    // Early exit: out of bounds
    let x_min = x0.min(x1);
    let x_max = x0.max(x1);
    if px < x_min - stroke_width || px > x_max + stroke_width {
        return false;
    }
    let y_min = y0.min(y1);
    let y_max = y0.max(y1);
    if py < y_min - stroke_width || py > y_max + stroke_width {
        return false;
    }

    // AB is the segment from (x0, y0) to (x1, y1) and P is (point_x, point_y).
    //  P
    //   .
    //    .
    //     A----->B
    // If AP dot AB is <= 0.0, then PA is pointing away from AB, so A is the closest point.
    let abx = x1 - x0;
    let aby = y1 - y0;
    let apx = px - x0;
    let apy = py - y0;
    let dot_a = abx * apx + aby * apy;
    let dist = if dot_a <= 0.0 {
        apx * apx + apy * apy
    } else {
        // If BP dot AB is >= 0.0, then BP is pointing away from BA, so B is the closest point.
        let bpx = px - x1;
        let bpy = py - y1;
        let dot_b = abx * bpx + aby * bpy;
        if dot_b >= 0.0 {
            bpx * bpx + bpy * bpy
        } else {
            // Otherwise, the closest point will be within the interval of the segment.
            // Project the point onto the segment.
            let len = abx * abx + aby * aby;
            let ex = apx - dot_a * abx / len;
            let ey = apy - dot_a * aby / len;
            ex * ex + ey * ey
        }
    };

    dist <= stroke_width_sq
}

/// Returns whether the given point is inside the stroked bezier curve.
/// `width_sq` should be the squared width of the stroke.
fn hit_test_stroke_curve(
    (point_x, point_y): (Twips, Twips),
    (x0, y0): (Twips, Twips),
    (x1, y1): (Twips, Twips),
    (x2, y2): (Twips, Twips),
    (stroke_width, stroke_width_sq): (f64, f64),
) -> bool {
    let px = point_x.get() as f64;
    let py = point_y.get() as f64;
    let x0 = x0.get() as f64;
    let y0 = y0.get() as f64;
    let x1 = x1.get() as f64;
    let y1 = y1.get() as f64;
    let x2 = x2.get() as f64;
    let y2 = y2.get() as f64;

    // Early exit: out of bounds
    // TODO: Since this involves an expensive cubic, probably wortwhile to calculate the tight bounds for the curve:
    // https://www.iquilezles.org/www/articles/bezierbbox/bezierbbox.htm
    let x_min = x0.min(x1).min(x2);
    let x_max = x0.max(x1).max(x2);
    if px < x_min - stroke_width || px > x_max + stroke_width {
        return false;
    }

    let y_min = y0.min(y1).min(y2);
    let y_max = y0.max(y1).max(y2);
    if py < y_min - stroke_width || py > y_max + stroke_width {
        return false;
    }

    // The closest point on the curve will be normal to the curve.
    // The tangent of a quadratic bezier:
    // C'(t) = -2 * (1-t) * P0 + 2 * (1-t) * P1 + 2*t*P2
    // Dot product to determine when we are perpendicular to the tangent.
    // (point - C(t)) . C'(t) = 0
    // The result is a cubic polynomial that we can solve for.
    // After solving this polynomial, we choose the t with [0, 1.0] that gives us the minimum distance
    // (also considering the endcaps).
    // via http://blog.gludion.com/2009/08/distance-to-quadratic-bezier-curve.html

    let ax = x1 - x0;
    let ay = y1 - y0;
    let bx = x2 - x1 - ax;
    let by = y2 - y1 - ay;
    let mx = x0 - px;
    let my = y0 - py;

    let a = bx * bx + by * by;
    let b = 3.0 * (ax * bx + ay * by);
    let c = 2.0 * (ax * ax + ay * ay) + (mx * bx + my * by);
    let d = mx * ax + my * ay;

    let distance_to_curve = |t| -> f64 {
        // Sample bezier at the given t and return distance to the point.
        let comp_t = 1.0 - t;
        let cx = comp_t * comp_t * x0 + 2.0 * comp_t * t * x1 + t * t * x2;
        let cy = comp_t * comp_t * y0 + 2.0 * comp_t * t * y1 + t * t * y2;
        let dx = cx - px;
        let dy = cy - py;
        dx * dx + dy * dy
    };

    // Test end-caps
    let mut dist = distance_to_curve(0.0);
    dist = dist.min(distance_to_curve(1.0));

    // Test roots.
    for t in solve_cubic(a, b, c, d) {
        if t >= 0.0 && t <= 1.0 {
            dist = dist.min(distance_to_curve(t));
        }
    }

    dist <= stroke_width_sq
}

/// Calculates the winding number for a line segment relative to the given point.
fn winding_number_line(
    (point_x, point_y): (Twips, Twips),
    (x0, y0): (Twips, Twips),
    (x1, y1): (Twips, Twips),
) -> i32 {
    let point_x = point_x.get() as f64;
    let point_y = point_y.get() as f64;
    let x0 = x0.get() as f64;
    let y0 = y0.get() as f64;
    let x1 = x1.get() as f64;
    let y1 = y1.get() as f64;

    // Adjust winding number if we are on the left side of the segment.
    // An upward segment (-y) increments the winding number (including the initial endpoint).
    // A downward segment (+y) decrements the winding number (including the final endpoint)
    // Perp-dot indicates which side of the segment the point is on.
    if y0 <= point_y {
        if y1 > point_y {
            let val = (x1 - x0) * (point_y - y0) - (y1 - y0) * (point_x - x0);
            if val > 0.0 {
                return 1;
            }
        }
    } else if y1 <= point_y {
        let val = (x1 - x0) * (point_y - y0) - (y1 - y0) * (point_x - x0);

        if val < 0.0 {
            return -1;
        }
    }

    0
}

/// Calculates the winding number for a bezier curve around the given point.
fn winding_number_curve(
    (point_x, point_y): (Twips, Twips),
    (ax0, ay0): (Twips, Twips),
    (ax1, ay1): (Twips, Twips),
    (ax2, ay2): (Twips, Twips),
) -> i32 {
    // Intersect a ray on the +x axis with the quadratic bezier.
    //
    // Translate so the test point and ray is at the origin.
    // The ray-curve intersection is solving the quadratic:
    // y_0*(1-t)^2 + y_1*2*t*(1-t) + y_2*t^2 = 0

    // However, there are two issues:
    // 1) Solving the quadratic needs to be numerically robust, particularly near the endpoints 0.0 and 1.0, and as the curve is tangent to the ray.
    //    We use the "Citardauq" method for improved numerical stability.
    // 2) The convention for including/excluding endpoints needs to act similarly to lines, with the initial point included if the curve is "downward",
    //    and the final point included if the curve is pointing "upward". This is complicated by the fact that the curve could be tangent to the ray
    //    at the endpoint (this is still considered "upward" or "downward" depending on the slope at earlier t).
    //    We solve this by splitting the curve into y-monotonic subcurves. This is helpful because
    //    a) each subcurve will have 1 intersection with the ray
    //    b) if the subcurve surrounds the ray, we know it has an intersection without having to check if t is in [0, 1]
    //    c) we can determine the winding of the segment upward/downward by comparing the subcurve endpoints, also properly handling the endpoint convention.

    let x0 = ax0.get() - point_x.get();
    let y0 = ay0.get() - point_y.get();
    let x1 = ax1.get() - point_x.get();
    let y1 = ay1.get() - point_y.get();
    let x2 = ax2.get() - point_x.get();
    let y2 = ay2.get() - point_y.get();

    // Early exit: all control points out of bounds.
    if (y0 < 0 && y1 < 0 && y2 < 0)
        || (y0 > 0 && y1 > 0 && y2 > 0)
        || (x0 <= 0 && x1 <= 0 && x2 <= 0)
    {
        return 0;
    }

    let x0 = x0 as f64;
    let y0 = y0 as f64;
    let x1 = x1 as f64;
    let y1 = y1 as f64;
    let x2 = x2 as f64;
    let y2 = y2 as f64;

    let a = y0 - 2.0 * y1 + y2;
    let b = 2.0 * (y1 - y0);
    let c = y0;

    let roots = solve_quadratic(a, b, c);

    if roots.is_empty() {
        return 0;
    }

    // Split the curve into two y-monotonic segments.
    let mut y_start = y0;
    let mut y_end = if roots.len() == 1 {
        // Linear, so already monotonic.
        y2
    } else {
        // Find the extrema point where the curve is horizontal.
        // This is the point that splits the curve into 2 y-monotonic segments.
        let t_extrema = -b / (2.0 * a);
        a * t_extrema * t_extrema + b * t_extrema + c
    };

    let ax = x0 - 2.0 * x1 + x2;
    let bx = 2.0 * (x1 - x0);

    let mut winding = 0;
    for t in roots {
        // Verify that this monotonic segment straddles the ray, and choose winding direction.
        // This also handles the endpoint conventions.
        let direction = if y_end > y_start {
            // Downward edge: initial point included, increments winding.
            if y_start > 0.0 || y_end <= 0.0 {
                0
            } else {
                1
            }
        } else if y_start > y_end {
            // Upward edge: final point included, increments winding.
            if y_start <= 0.0 || y_end > 0.0 {
                0
            } else {
                -1
            }
        } else {
            0
        };

        // If curve point is to the right of the ray origin, the ray will hit it.
        // We don't have to do the problematic 0 <= t <= 1 check because this vertical slice is guaranteed
        // to contain the monotonic segment, and our roots are returned in order by `solve_quadratic`.
        // Adjust the winding as appropriate.
        if direction != 0 {
            let t_x = x0 + bx * t + ax * t * t;
            if t_x > 0.0 {
                winding += direction;
            }
        }

        // Advance to next monotonic segment
        y_start = y_end;
        y_end = y2;
    }

    winding
}

const COEFFICIENT_EPSILON: f64 = 0.0000001;

/// Returns the roots of the quadratic ax^2 + bx + c = 0.
/// The roots may not be unique.
/// Uses the "Citardauq" formula for numerical stability.
/// Originally from https://github.com/linebender/kurbo/blob/master/src/common.rs
/// See https://math.stackexchange.com/questions/866331
fn solve_quadratic(a: f64, b: f64, c: f64) -> SmallVec<[f64; 2]> {
    let mut result = SmallVec::new();
    let sc0 = c * a.recip();
    let sc1 = b * a.recip();
    if !sc0.is_finite() || !sc1.is_finite() {
        // c2 is zero or very small, treat as linear eqn
        let root = -c / b;
        if root.is_finite() {
            result.push(root);
        }
        return result;
    }
    let arg = sc1 * sc1 - 4.0 * sc0;
    let root1 = if !arg.is_finite() {
        // Likely, calculation of sc1 * sc1 overflowed. Find one root
        // using sc1 x + x² = 0, other root as sc0 / root1.
        -sc1
    } else {
        if arg < 0.0 {
            return result;
        }
        -0.5 * (sc1 + arg.sqrt().copysign(sc1))
    };
    let root2 = sc0 / root1;
    // Sort just to be friendly and make results deterministic.
    if root2 > root1 {
        result.push(root1);
        result.push(root2);
    } else {
        result.push(root2);
        result.push(root1);
    }
    result
}

/// Returns the roots of a cubic polynomial, ax^3 + bx^2 + cx + d = 0
/// from http://www.cplusplus.com/forum/beginner/234717/
/// The roots are not necessarily unique.
/// TODO: This probably isn't numerically robust
#[allow(clippy::many_single_char_names)]
fn solve_cubic(a: f64, b: f64, c: f64, d: f64) -> SmallVec<[f64; 3]> {
    let mut roots = SmallVec::new();

    if a.abs() <= COEFFICIENT_EPSILON {
        // Fall back to quadratic formula.
        roots.extend_from_slice(&solve_quadratic(b, c, d));
        return roots;
    }

    // Reduce to a "depressed cubic", x^3 + px + q = 0
    // https://en.wikipedia.org/wiki/Cubic_equation#Cardano's_formula
    let p = (b * b - 3.0 * a * c) / (9.0 * a * a);
    let q = (9.0 * a * b * c - 27.0 * a * a * d - 2.0 * b * b * b) / (54.0 * a * a * a);
    let offset = b / (3.0 * a);
    let disc = p * p * p - q * q;

    // The discriminant determines the number of real roots.
    if disc > 0.0 {
        let theta = f64::acos(q / (p * f64::sqrt(p)));
        let r = 2.0 * f64::sqrt(p);
        let t0 = r * f64::cos(theta / 3.0) - offset;
        let t1 = r * f64::cos((theta + 2.0 * std::f64::consts::PI) / 3.0) - offset;
        let t2 = r * f64::cos((theta + 4.0 * std::f64::consts::PI) / 3.0) - offset;
        roots.push(t0);
        roots.push(t1);
        roots.push(t2);
    } else {
        let gamma1 = f64::cbrt(q + f64::sqrt(-disc));
        let gamma2 = f64::cbrt(q - f64::sqrt(-disc));

        let t0 = gamma1 + gamma2 - offset;
        let t1 = -0.5 * (gamma1 + gamma2) - offset;
        if disc == 0.0 {
            roots.push(t0);
            roots.push(t1);
        } else {
            roots.push(t0);
        }
    }

    roots
}

/// Converts an SWF glyph into an SWF shape, for ease of use by rendering backends.
pub fn swf_glyph_to_shape(glyph: &swf::Glyph) -> swf::Shape {
    // Per SWF19 p.164, the FontBoundsTable can contain empty bounds for every glyph (reserved).
    // SWF19 says this is true through SWFv7, but it seems like it might be generally true?
    // In any case, we have to be sure to calculate the shape bounds ourselves to make a proper
    // SVG.
    let bounds = glyph
        .bounds
        .clone()
        .filter(|b| b.x_min != b.x_max || b.y_min != b.y_max)
        .unwrap_or_else(|| calculate_shape_bounds(&glyph.shape_records[..]).into());
    swf::Shape {
        version: 2,
        id: 0,
        shape_bounds: bounds.clone(),
        edge_bounds: bounds,
        has_fill_winding_rule: false,
        has_non_scaling_strokes: false,
        has_scaling_strokes: true,
        styles: swf::ShapeStyles {
            fill_styles: vec![swf::FillStyle::Color(swf::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            })],
            line_styles: vec![],
        },
        shape: glyph.shape_records.clone(),
    }
}
