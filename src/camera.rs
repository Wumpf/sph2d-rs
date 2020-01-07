use super::units::*;
use ggez::nalgebra as na;
use ggez::graphics::Rect;

// A 2D camera.
// Maps 2D world coordinates/sizes to screen coordinates/sizes.
//
// 2D World: ↑ y → x, origin bottom left
// This camera does not allow for non-uniform scaling.
#[derive(PartialEq, Debug)]
pub struct Camera {
    pub screen: Rect, // Screen rectangle
    pub pixel_per_world_unit: f32, // Scaling/Zoom factor of the camera ()
    pub position: Position, // The position of this camera in world space, i.e. the middle of the view.
}

impl Camera {
    // todo: ensure that a given rectangle of world units is visible
    pub fn center_around_world_rect(screen: Rect, world_rect_to_fit: Rect) -> Camera {
        let screen_extent = Size::new(screen.w, screen.h.abs());
        let world_extent = Size::new(world_rect_to_fit.w, world_rect_to_fit.h);
        let pixel_per_world_unit2d = screen_extent.component_div(&world_extent);
        let world_rect_center = Position::new(
            world_rect_to_fit.x + world_rect_to_fit.w * 0.5,
            world_rect_to_fit.y + world_rect_to_fit.h * 0.5,
        );

        Camera {
            screen: screen,
            pixel_per_world_unit: pixel_per_world_unit2d.x.min(pixel_per_world_unit2d.y),
            position: world_rect_center,
        }
    }

    pub fn world_unit_scale(&self) -> Size {
        Size::new(self.pixel_per_world_unit, self.pixel_per_world_unit)
    }

    pub fn world_to_screen_coords(&self, world_pos: Position) -> Position {
        let from_camera = world_pos - self.position;
        let view_scale = from_camera * self.pixel_per_world_unit;

        assert!(self.screen.w > 0.0 && self.screen.h > 0.0);

        Position::new(
            self.screen.x + view_scale.x + self.screen.w * 0.5,
            self.screen.y - view_scale.y + self.screen.h * 0.5,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_from_world_rect() {
        let screen = Rect::new(321.0, 123.0, 200.0, 100.0);
        let world = Rect::new(10.0, 10.0, 20.0, 40.0);
        let camera = Camera::center_around_world_rect(screen, world);
        assert_eq!(Camera {
            screen: Rect::new(321.0, 123.0, 200.0, 100.0),
            pixel_per_world_unit: 2.5,
            position: Position::new(20.0, 30.0),
        }, camera);
    }

    #[test]
    fn world_to_screen_conversion() {
        // position at origin, screen no offset
        {
            let camera = Camera {
                screen: Rect::new(0.0, 0.0, 200.0, 100.0),
                pixel_per_world_unit: 10.0,
                position: Position::origin(),
            };

            assert_eq!(camera.world_to_screen_coords(Position::origin()), Position::new(100.0, 50.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(1.0, 1.0)), Position::new(110.0, 40.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(-1.0, -1.0)), Position::new(90.0, 60.0));
        }

        // position at offset, screen no offset
        {
            let camera = Camera {
                screen: Rect::new(0.0, 0.0, 200.0, 100.0),
                pixel_per_world_unit: 10.0,
                position: Position::new(1.0, 1.0),
            };
            assert_eq!(camera.world_to_screen_coords(Position::origin()), Position::new(90.0, 60.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(1.0, 1.0)), Position::new(100.0, 50.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(-1.0, -1.0)), Position::new(80.0, 70.0));
        }

        // position at origin, screen with some offset
        {
            let camera = Camera {
                screen: Rect::new(1.0, 2.0, 200.0, 100.0),
                pixel_per_world_unit: 10.0,
                position: Position::origin(),
            };

            assert_eq!(camera.world_to_screen_coords(Position::origin()), Position::new(101.0, 52.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(1.0, 1.0)), Position::new(111.0, 42.0));
            assert_eq!(camera.world_to_screen_coords(Position::new(-1.0, -1.0)), Position::new(91.0, 62.0));
        }
    }
}
