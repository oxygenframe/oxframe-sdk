//! Pictures from the software renderer.

use oxframe_3d::{Light, Mesh, Scene3d, Vec3};
use oxframe_core::Color;

fn px(f: &oxframe_3d::Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

fn red_cube() -> Scene3d {
    let world = Scene3d::new();
    world.add(Mesh::cube().color(Color::rgb(0.9, 0.1, 0.1)));
    world
}

#[test]
fn a_cube_in_the_middle_on_a_transparent_background() {
    let f = red_cube().render(80, 60, 2);
    assert_eq!((f.width, f.height, f.rgba.len()), (80, 60, 80 * 60 * 4));
    assert_eq!(px(&f, 0, 0)[3], 0, "background is transparent");
    let c = px(&f, 40, 30);
    assert_eq!(c[3], 255);
    assert!(c[0] > 100 && c[0] > c[1] * 2 && c[0] > c[2] * 2, "{c:?}");
}

#[test]
fn the_same_scene_gives_the_same_pixels() {
    let a = red_cube().render(64, 48, 2);
    let b = red_cube().render(64, 48, 2);
    assert_eq!(a, b);
    let world = red_cube();
    assert_eq!(world.fingerprint(64, 48, 2), world.fingerprint(64, 48, 2));
    let before = world.fingerprint(64, 48, 2);
    world.camera().fov().set(60.0);
    assert_ne!(world.fingerprint(64, 48, 2), before);
}

#[test]
fn nearer_objects_hide_farther_ones() {
    let world = Scene3d::new();
    world.camera().position().set(Vec3::new(0.0, 0.0, 5.0));
    world.add(Mesh::cube().color(Color::rgb(0.9, 0.1, 0.1)));
    // A green sphere between the camera and the cube.
    world.add(
        Mesh::sphere()
            .color(Color::rgb(0.1, 0.9, 0.1))
            .position((0.0, 0.0, 2.0)),
    );
    let f = world.render(60, 60, 1);
    let c = px(&f, 30, 30);
    assert!(c[1] > c[0], "{c:?}");
    // Moving the sphere behind the cube shows the cube.
    world.add(Mesh::group());
    let world = Scene3d::new();
    world.camera().position().set(Vec3::new(0.0, 0.0, 5.0));
    world.add(Mesh::cube().color(Color::rgb(0.9, 0.1, 0.1)));
    world.add(
        Mesh::sphere()
            .color(Color::rgb(0.1, 0.9, 0.1))
            .position((0.0, 0.0, -2.0)),
    );
    let c = px(&world.render(60, 60, 1), 30, 30);
    assert!(c[0] > c[1], "{c:?}");
}

#[test]
fn children_follow_their_parent_and_lights_change_shading() {
    let world = Scene3d::new();
    world.camera().position().set(Vec3::new(0.0, 0.0, 6.0));
    let arm = world.add(Mesh::group());
    arm.add(
        Mesh::cube()
            .scale(0.5)
            .position((1.5, 0.0, 0.0))
            .color(Color::WHITE),
    );
    let right = world.render(80, 40, 1);
    arm.rotation().set(Vec3::new(0.0, 0.0, 180.0));
    let left = world.render(80, 40, 1);
    // The cube swung from the right half to the left half.
    let filled = |f: &oxframe_3d::Frame, x0: u32, x1: u32| (x0..x1).any(|x| px(f, x, 20)[3] > 0);
    assert!(filled(&right, 41, 80) && !filled(&right, 0, 39));
    assert!(filled(&left, 0, 39) && !filled(&left, 41, 80));

    let dim = Scene3d::new();
    dim.add(Mesh::cube());
    dim.light(Light::directional((-1.0, -2.0, -1.5)).intensity(0.2));
    let bright = Scene3d::new();
    bright.add(Mesh::cube());
    bright.light(Light::directional((-1.0, -2.0, -1.5)).intensity(2.0));
    let (d, b) = (
        px(&dim.render(40, 30, 1), 20, 15),
        px(&bright.render(40, 30, 1), 20, 15),
    );
    assert!(b[0] > d[0], "{d:?} {b:?}");
}

/// Writes a picture for eyeballing: `OXFRAME_3D_PNG=out.png cargo test -p
/// oxframe-3d --test render -- --ignored`.
#[test]
#[ignore = "writes a PNG to look at"]
fn picture() {
    let Ok(path) = std::env::var("OXFRAME_3D_PNG") else {
        return;
    };
    let world = Scene3d::new();
    world.add(
        Mesh::plane()
            .scale(6.0)
            .position((0.0, -0.75, 0.0))
            .color(Color::rgb(0.3, 0.32, 0.36)),
    );
    world.add(
        Mesh::cube()
            .color(Color::rgb(0.38, 0.6, 0.95))
            .rotation((0.0, 30.0, 0.0)),
    );
    world.add(
        Mesh::sphere()
            .position((-1.4, -0.25, 0.6))
            .color(Color::rgb(0.95, 0.75, 0.2))
            .metallic(1.0)
            .roughness(0.3),
    );
    world.add(
        Mesh::cylinder()
            .position((1.5, -0.25, 0.4))
            .scale(0.8)
            .color(Color::rgb(0.9, 0.2, 0.3))
            .roughness(0.8),
    );
    let f = world.render(960, 540, 2);
    let img = oxframe_core::Image {
        width: f.width,
        height: f.height,
        data: f.rgba,
    };
    std::fs::write(path, img.encode_png()).unwrap();
}
