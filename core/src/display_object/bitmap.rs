//! Bitmap display object

use crate::backend::render::BitmapHandle;
use crate::context::{RenderContext, UpdateContext};
use crate::display_object::{DisplayObjectBase, TDisplayObject};
use crate::prelude::*;
use crate::types::{Degrees, Percent};
use gc_arena::{Collect, Gc, GcCell};

/// A Bitmap display object is a raw bitamp on the stage.
/// This can only be instanitated on the display list in SWFv9 AVM2 files.
/// In AVM1, this is only a library symbol that is referenced by `Graphic`.
/// Normally bitmaps are drawn in Flash as part of a Shape tag (`Graphic`),
/// but starting in AVM2, a raw `Bitmap` display object can be created
/// with the `PlaceObject3` tag.
/// It can also be created in ActionScript using the `Bitmap` class.
#[derive(Clone, Debug, Collect, Copy)]
#[collect(no_drop)]
pub struct Bitmap<'gc>(GcCell<'gc, BitmapData<'gc>>);

#[derive(Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct BitmapData<'gc> {
    base: DisplayObjectBase<'gc>,
    static_data: Gc<'gc, BitmapStatic>,
    bitmap_data: Option<GcCell<'gc, crate::avm1::object::bitmap_data::BitmapData>>,
    smoothing: bool,
}

impl<'gc> Bitmap<'gc> {
    pub fn new_with_bitmap_data(
        context: &mut UpdateContext<'_, 'gc, '_>,
        id: CharacterId,
        bitmap_handle: BitmapHandle,
        width: u16,
        height: u16,
        bitmap_data: Option<GcCell<'gc, crate::avm1::object::bitmap_data::BitmapData>>,
        smoothing: bool,
    ) -> Self {
        Bitmap(GcCell::allocate(
            context.gc_context,
            BitmapData {
                base: Default::default(),
                static_data: Gc::allocate(
                    context.gc_context,
                    BitmapStatic {
                        id,
                        bitmap_handle,
                        width,
                        height,
                    },
                ),
                bitmap_data,
                smoothing,
            },
        ))
    }

    pub fn new(
        context: &mut UpdateContext<'_, 'gc, '_>,
        id: CharacterId,
        bitmap_handle: BitmapHandle,
        width: u16,
        height: u16,
    ) -> Self {
        Self::new_with_bitmap_data(context, id, bitmap_handle, width, height, None, true)
    }

    #[allow(dead_code)]
    pub fn bitmap_handle(self) -> BitmapHandle {
        self.0.read().static_data.bitmap_handle
    }

    pub fn width(self) -> u16 {
        self.0.read().static_data.width
    }

    pub fn height(self) -> u16 {
        self.0.read().static_data.height
    }
}

impl<'gc> TDisplayObject<'gc> for Bitmap<'gc> {
    impl_display_object!(base);

    fn id(&self) -> CharacterId {
        self.0.read().static_data.id
    }

    fn self_bounds(&self) -> BoundingBox {
        BoundingBox {
            x_min: Twips::new(0),
            y_min: Twips::new(0),
            x_max: Twips::from_pixels(Bitmap::width(*self).into()),
            y_max: Twips::from_pixels(Bitmap::height(*self).into()),
        }
    }

    fn run_frame(&self, context: &mut UpdateContext<'_, 'gc, '_>) {
        if let Some(bitmap_data) = &self.0.read().bitmap_data {
            let bd = bitmap_data.read();
            if bd.dirty() {
                let _ = context.renderer.update_texture(
                    self.0.read().static_data.bitmap_handle,
                    bd.width(),
                    bd.height(),
                    bd.pixels_rgba(),
                );
                drop(bd);
                bitmap_data.write(context.gc_context).set_dirty(false);
            }
        }
    }

    fn render_self(&self, context: &mut RenderContext) {
        if !self.world_bounds().intersects(&context.view_bounds) {
            // Off-screen; culled
            return;
        }

        let bitmap_data = self.0.read();
        context.renderer.render_bitmap(
            bitmap_data.static_data.bitmap_handle,
            context.transform_stack.transform(),
            bitmap_data.smoothing,
        );
    }
}

/// Static data shared between all instances of a bitmap.
#[derive(Clone, Collect)]
#[collect(no_drop)]
struct BitmapStatic {
    id: CharacterId,
    bitmap_handle: BitmapHandle,
    width: u16,
    height: u16,
}
