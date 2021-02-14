use crate::context::{RenderContext, UpdateContext};
use crate::display_object::{DisplayObjectBase, TDisplayObject};
use crate::font::TextRenderSettings;
use crate::prelude::*;
use crate::tag_utils::SwfMovie;
use crate::transform::Transform;
use crate::types::{Degrees, Percent};
use gc_arena::{Collect, GcCell, MutationContext};
use std::sync::Arc;

#[derive(Clone, Debug, Collect, Copy)]
#[collect(no_drop)]
pub struct Text<'gc>(GcCell<'gc, TextData<'gc>>);

#[derive(Clone, Debug)]
pub struct TextData<'gc> {
    base: DisplayObjectBase<'gc>,
    static_data: gc_arena::Gc<'gc, TextStatic>,
    render_settings: TextRenderSettings,
}

impl<'gc> Text<'gc> {
    pub fn from_swf_tag(
        context: &mut UpdateContext<'_, 'gc, '_>,
        swf: Arc<SwfMovie>,
        tag: &swf::Text,
    ) -> Self {
        Text(GcCell::allocate(
            context.gc_context,
            TextData {
                base: Default::default(),
                static_data: gc_arena::Gc::allocate(
                    context.gc_context,
                    TextStatic {
                        swf,
                        id: tag.id,
                        bounds: tag.bounds.clone().into(),
                        text_transform: tag.matrix,
                        text_blocks: tag.records.clone(),
                    },
                ),
                render_settings: Default::default(),
            },
        ))
    }

    pub fn set_render_settings(
        self,
        gc_context: MutationContext<'gc, '_>,
        settings: TextRenderSettings,
    ) {
        self.0.write(gc_context).render_settings = settings
    }
}

impl<'gc> TDisplayObject<'gc> for Text<'gc> {
    impl_display_object!(base);

    fn id(&self) -> CharacterId {
        self.0.read().static_data.id
    }

    fn movie(&self) -> Option<Arc<SwfMovie>> {
        Some(self.0.read().static_data.swf.clone())
    }

    fn run_frame(&self, _context: &mut UpdateContext) {
        // Noop
    }

    fn render_self(&self, context: &mut RenderContext) {
        let tf = self.0.read();
        context.transform_stack.push(&Transform {
            matrix: tf.static_data.text_transform,
            ..Default::default()
        });

        let mut color = swf::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        let mut font_id = 0;
        let mut height = Twips::new(0);
        let mut transform: Transform = Default::default();
        for block in &tf.static_data.text_blocks {
            if let Some(x) = block.x_offset {
                transform.matrix.tx = x;
            }
            if let Some(y) = block.y_offset {
                transform.matrix.ty = y;
            }
            color = block.color.as_ref().unwrap_or(&color).clone();
            font_id = block.font_id.unwrap_or(font_id);
            height = block.height.unwrap_or(height);
            if let Some(font) = context
                .library
                .library_for_movie(self.movie().unwrap())
                .unwrap()
                .get_font(font_id)
            {
                let scale = (height.get() as f32) / font.scale();
                transform.matrix.a = scale;
                transform.matrix.d = scale;
                transform.color_transform.r_mult = f32::from(color.r) / 255.0;
                transform.color_transform.g_mult = f32::from(color.g) / 255.0;
                transform.color_transform.b_mult = f32::from(color.b) / 255.0;
                transform.color_transform.a_mult = f32::from(color.a) / 255.0;
                for c in &block.glyphs {
                    if let Some(glyph) = font.get_glyph(c.index as usize) {
                        context.transform_stack.push(&transform);
                        context
                            .renderer
                            .render_shape(glyph.shape_handle, context.transform_stack.transform());
                        context.transform_stack.pop();
                        transform.matrix.tx += Twips::new(c.advance);
                    }
                }
            }
        }
        context.transform_stack.pop();
    }

    fn self_bounds(&self, _with_stroke: bool) -> BoundingBox {
        self.0.read().static_data.bounds.clone()
    }

    fn hit_test_shape(
        &self,
        context: &mut UpdateContext<'_, 'gc, '_>,
        mut point: (Twips, Twips),
    ) -> bool {
        if self.world_shape_bounds().contains(point) {
            // Texts using the "Advanced text rendering" always hit test using their bounding box.
            if self.0.read().render_settings.is_advanced() {
                return true;
            }

            // Transform the point into the text's local space.
            let local_matrix = self.global_to_local_matrix();
            let tf = self.0.read();
            let mut text_matrix = tf.static_data.text_transform;
            text_matrix.invert();
            point = text_matrix * local_matrix * point;

            let mut font_id = 0;
            let mut height = Twips::new(0);
            let mut glyph_matrix = Matrix::default();
            for block in &tf.static_data.text_blocks {
                if let Some(x) = block.x_offset {
                    glyph_matrix.tx = x;
                }
                if let Some(y) = block.y_offset {
                    glyph_matrix.ty = y;
                }
                font_id = block.font_id.unwrap_or(font_id);
                height = block.height.unwrap_or(height);

                if let Some(font) = context
                    .library
                    .library_for_movie(self.movie().unwrap())
                    .unwrap()
                    .get_font(font_id)
                {
                    let scale = (height.get() as f32) / font.scale();
                    glyph_matrix.a = scale;
                    glyph_matrix.d = scale;
                    for c in &block.glyphs {
                        if let Some(glyph) = font.get_glyph(c.index as usize) {
                            // Transform the point into glyph space and test.
                            let mut matrix = glyph_matrix;
                            matrix.invert();
                            let point = matrix * point;
                            let glyph_bounds = BoundingBox::from(&glyph.shape.shape_bounds);
                            if glyph_bounds.contains(point)
                                && crate::shape_utils::shape_hit_test(
                                    &glyph.shape,
                                    point,
                                    &local_matrix,
                                )
                            {
                                return true;
                            }

                            glyph_matrix.tx += Twips::new(c.advance);
                        }
                    }
                }
            }
        }

        false
    }
}

unsafe impl<'gc> gc_arena::Collect for TextData<'gc> {
    #[inline]
    fn trace(&self, cc: gc_arena::CollectionContext) {
        self.base.trace(cc);
        self.static_data.trace(cc);
    }
}

/// Static data shared between all instances of a text object.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct TextStatic {
    swf: Arc<SwfMovie>,
    id: CharacterId,
    bounds: BoundingBox,
    text_transform: Matrix,
    text_blocks: Vec<swf::TextRecord>,
}

unsafe impl<'gc> gc_arena::Collect for TextStatic {
    #[inline]
    fn needs_trace() -> bool {
        false
    }
}
