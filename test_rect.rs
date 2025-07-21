use bevy::math::{Rect, Vec2};

fn main() {
    let rect = Rect::new(0.0, 0.0, 256.0, 256.0);
    println\!("Rect: {:?}", rect);
    println\!("Contains (256.0, 256.0): {}", rect.contains(Vec2::new(256.0, 256.0)));
    println\!("Contains (255.9, 255.9): {}", rect.contains(Vec2::new(255.9, 255.9)));
    println\!("Contains (0.0, 0.0): {}", rect.contains(Vec2::new(0.0, 0.0)));
}
EOF < /dev/null