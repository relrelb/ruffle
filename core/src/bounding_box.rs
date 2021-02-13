use gc_arena::Collect;
use swf::{Matrix, Rectangle, Twips};

#[derive(Clone, Debug, PartialEq, Collect)]
#[collect(require_static)]
pub struct BoundingBox {
    pub x_min: Twips,
    pub x_max: Twips,
    pub y_min: Twips,
    pub y_max: Twips,
}

impl BoundingBox {
    #[inline]
    fn valid(&self) -> bool {
        self.x_max >= self.x_min && self.y_max >= self.y_min
    }

    /// Clamp the given point inside this bounding box.
    pub fn clamp(&self, (x, y): (Twips, Twips)) -> (Twips, Twips) {
        if !self.valid() {
            return (x, y);
        }

        (
            x.clamp(self.x_min, self.x_max),
            y.clamp(self.y_min, self.y_max),
        )
    }

    pub fn transform(&self, matrix: &Matrix) -> Self {
        if !self.valid() {
            return Self::default();
        }

        let (x0, y0) = *matrix * (self.x_min, self.y_min);
        let (x1, y1) = *matrix * (self.x_min, self.y_max);
        let (x2, y2) = *matrix * (self.x_max, self.y_min);
        let (x3, y3) = *matrix * (self.x_max, self.y_max);
        BoundingBox {
            x_min: x0.min(x1).min(x2).min(x3),
            x_max: x0.max(x1).max(x2).max(x3),
            y_min: y0.min(y1).min(y2).min(y3),
            y_max: y0.max(y1).max(y2).max(y3),
        }
    }

    pub fn encompass(&mut self, (x, y): (Twips, Twips)) {
        if !self.valid() {
            self.x_min = x;
            self.x_max = x;
            self.y_min = y;
            self.y_max = y;
            return;
        }

        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }

    pub fn union(&mut self, other: &BoundingBox) {
        if !other.valid() {
            return;
        }
        if !self.valid() {
            *self = other.clone();
            return;
        }

        self.x_min = self.x_min.min(other.x_min);
        self.x_max = self.x_max.max(other.x_max);
        self.y_min = self.y_min.min(other.y_min);
        self.y_max = self.y_max.max(other.y_max);
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        if !self.valid() || !other.valid() {
            return false;
        }

        self.x_min <= other.x_max
            && self.x_max >= other.x_min
            && self.y_min <= other.y_max
            && self.y_max >= other.y_min
    }

    pub fn contains(&self, (x, y): (Twips, Twips)) -> bool {
        if !self.valid() {
            return false;
        }

        x >= self.x_min && x <= self.x_max && y >= self.y_min && y <= self.y_max
    }

    /// Set the X coordinate to a particular value, maintaining the width of
    /// this bounding box.
    pub fn set_x(&mut self, x: Twips) {
        let width = self.width();
        self.x_min = x;
        self.x_max = x + width;
    }

    /// Set the Y coordinate to a particular value, maintaining the height of
    /// this bounding box.
    pub fn set_y(&mut self, y: Twips) {
        let height = self.height();
        self.y_min = y;
        self.y_max = y + height;
    }

    /// Determine the width of this bounding box.
    pub fn width(&self) -> Twips {
        if !self.valid() {
            return Default::default();
        }

        self.x_max - self.x_min
    }

    /// Adjust the width of this bounding box.
    pub fn set_width(&mut self, width: Twips) {
        self.x_max = self.x_min + width;
    }

    /// Determine the height of this bounding box.
    pub fn height(&self) -> Twips {
        if !self.valid() {
            return Default::default();
        }

        self.y_max - self.y_min
    }

    /// Adjust the height of this bounding box.
    pub fn set_height(&mut self, height: Twips) {
        self.y_max = self.y_min + height;
    }
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self {
            x_min: Twips::new(i32::MAX),
            x_max: Twips::new(i32::MIN),
            y_min: Twips::new(i32::MAX),
            y_max: Twips::new(i32::MIN),
        }
    }
}

impl From<Rectangle> for BoundingBox {
    fn from(rect: Rectangle) -> Self {
        Self {
            x_min: rect.x_min,
            x_max: rect.x_max,
            y_min: rect.y_min,
            y_max: rect.y_max,
        }
    }
}

impl From<&Rectangle> for BoundingBox {
    fn from(rect: &Rectangle) -> Self {
        Self {
            x_min: rect.x_min,
            x_max: rect.x_max,
            y_min: rect.y_min,
            y_max: rect.y_max,
        }
    }
}
