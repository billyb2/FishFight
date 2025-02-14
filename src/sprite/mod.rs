use macroquad::color;
use macroquad::experimental::collections::storage;
use macroquad::prelude::*;
use std::collections::HashMap;
use std::iter::FromIterator;
use std::ops::Div;

use hecs::World;

use serde::{Deserialize, Serialize};

use crate::json;
use crate::{Resources, Transform};

mod animated;

pub use animated::*;

/// This is used to specify draw order on a sprite
/// This will be used, primarily, by `Player` to draw equipped items in the right order, relative
/// to its own sprite. This is done by multiplying the player id by ten and adding whatever offset
/// is required to this number, to order it relative to other sprites controlled by this specific
/// `Player` component.
pub struct DrawOrder(pub u32);

/// Parameters for `Sprite` component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteMetadata {
    /// The id of the texture that will be used
    #[serde(rename = "texture")]
    pub texture_id: String,
    /// The sprites index in the sprite sheet
    #[serde(default)]
    pub index: usize,
    /// This is a scale factor that the sprite size will be multiplied by before draw
    #[serde(default)]
    pub scale: Option<f32>,
    /// The offset of the drawn sprite, relative to the position provided as an argument to the
    /// `Sprite` draw method.
    /// Note that this offset will not be inverted if the sprite is flipped.
    #[serde(default, with = "json::vec2_def")]
    pub offset: Vec2,
    /// The pivot of the sprite, relative to the position provided as an argument to the `Sprite`
    /// draw method, plus any offset.
    /// Note that this offset will not be inverted if the sprite is flipped.
    #[serde(
        default,
        with = "json::vec2_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub pivot: Option<Vec2>,
    /// The size of the drawn sprite. If no size is specified, the texture entry's `sprite_size`
    /// will be used, if specified, or the raw texture size, if not.
    #[serde(
        default,
        with = "json::vec2_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub size: Option<Vec2>,
    /// An optional color to blend with the texture color
    #[serde(
        default,
        with = "json::color_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub tint: Option<Color>,
    /// If this is true, the sprite will not be drawn.
    #[serde(default)]
    pub is_deactivated: bool,
}

impl Default for SpriteMetadata {
    fn default() -> Self {
        SpriteMetadata {
            texture_id: "".to_string(),
            index: 0,
            scale: None,
            offset: Vec2::ZERO,
            pivot: None,
            size: None,
            tint: None,
            is_deactivated: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sprite {
    pub texture: Texture2D,
    pub source_rect: Rect,
    pub tint: Color,
    pub scale: f32,
    pub offset: Vec2,
    pub pivot: Option<Vec2>,
    pub is_flipped_x: bool,
    pub is_flipped_y: bool,
    pub is_deactivated: bool,
}

impl Sprite {
    pub fn size(&self) -> Vec2 {
        self.source_rect.size() * self.scale
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }
}

impl From<SpriteMetadata> for Sprite {
    fn from(params: SpriteMetadata) -> Self {
        let texture_res = {
            let resources = storage::get::<Resources>();
            resources
                .textures
                .get(&params.texture_id)
                .cloned()
                .unwrap_or_else(|| panic!("Sprite: Invalid texture ID '{}'", &params.texture_id))
        };

        let source_rect = {
            let sprite_size = params.size.unwrap_or_else(|| texture_res.frame_size());

            let grid_size = texture_res.meta.size.div(sprite_size).as_u32();

            {
                let frame_cnt = (grid_size.x * grid_size.y) as usize;
                assert!(
                    params.index < frame_cnt,
                    "Sprite: index '{}' exceeds total frame count '{}'",
                    params.index,
                    frame_cnt
                );
            }

            let position = vec2(
                (params.index as u32 % grid_size.x) as f32 * sprite_size.x,
                (params.index as u32 / grid_size.x) as f32 * sprite_size.y,
            );

            Rect::new(position.x, position.y, sprite_size.x, sprite_size.y)
        };

        let tint = params.tint.unwrap_or(color::WHITE);

        Sprite {
            texture: texture_res.texture,
            source_rect,
            tint,
            scale: params.scale.unwrap_or(1.0),
            offset: params.offset,
            pivot: params.pivot,
            is_flipped_x: false,
            is_flipped_y: false,
            is_deactivated: params.is_deactivated,
        }
    }
}

pub fn draw_one_sprite(transform: &Transform, sprite: &Sprite) {
    if !sprite.is_deactivated {
        let size = sprite.size();

        draw_texture_ex(
            sprite.texture,
            transform.position.x + sprite.offset.x,
            transform.position.y + sprite.offset.y,
            sprite.tint,
            DrawTextureParams {
                flip_x: sprite.is_flipped_x,
                flip_y: sprite.is_flipped_y,
                rotation: transform.rotation,
                source: Some(sprite.source_rect),
                dest_size: Some(size),
                pivot: sprite.pivot,
            },
        );
    }
}

pub fn debug_draw_one_sprite(position: Vec2, sprite: &Sprite) {
    if !sprite.is_deactivated {
        let size = sprite.size();

        draw_rectangle_lines(
            position.x + sprite.offset.x,
            position.y + sprite.offset.y,
            size.x,
            size.y,
            2.0,
            color::BLUE,
        )
    }
}

#[derive(Debug)]
pub struct SpriteSet {
    pub draw_order: Vec<String>,
    pub map: HashMap<String, Sprite>,
}

impl From<&[(&str, Sprite)]> for SpriteSet {
    fn from(sprites: &[(&str, Sprite)]) -> Self {
        let draw_order = sprites.iter().map(|(id, _)| id.to_string()).collect();

        let map = HashMap::from_iter(
            sprites
                .iter()
                .cloned()
                .map(|(id, sprite)| (id.to_string(), sprite)),
        );

        SpriteSet { draw_order, map }
    }
}

impl SpriteSet {
    pub fn is_empty(&self) -> bool {
        self.draw_order.is_empty()
    }

    pub fn flip_all_x(&mut self, state: bool) {
        for sprite in self.map.values_mut() {
            sprite.is_flipped_x = state;
        }
    }

    pub fn flip_all_y(&mut self, state: bool) {
        for sprite in self.map.values_mut() {
            sprite.is_flipped_y = state;
        }
    }

    pub fn activate_all(&mut self) {
        for sprite in self.map.values_mut() {
            sprite.is_deactivated = false;
        }
    }

    pub fn deactivate_all(&mut self) {
        for sprite in self.map.values_mut() {
            sprite.is_deactivated = true;
        }
    }
}

pub fn draw_sprites(world: &mut World) {
    let mut ordered = world
        .query::<&DrawOrder>()
        .iter()
        .map(|(e, order)| (e, order.0))
        .collect::<Vec<_>>();

    ordered.sort_by(|&(_, a), &(_, b)| a.cmp(&b));

    // This needs to be performance tested. If it causes any issues, we can work around having to
    // do all these queries by wrapping all drawables in a `Drawable` enum type and match on that
    // in stead.
    for e in ordered.into_iter().map(|(e, _)| e) {
        if let Ok(transform) = world.get::<Transform>(e) {
            if let Ok(sprite) = world.get::<Sprite>(e) {
                draw_one_sprite(&transform, &sprite);
            } else if let Ok(sprite_set) = world.get::<SpriteSet>(e) {
                for id in sprite_set.draw_order.iter() {
                    let sprite = sprite_set.map.get(id).unwrap();
                    draw_one_sprite(&transform, sprite);
                }
            } else if let Ok(sprite) = world.get::<AnimatedSprite>(e) {
                draw_one_animated_sprite(&transform, &sprite);
            } else if let Ok(sprite_set) = world.get::<AnimatedSpriteSet>(e) {
                for id in sprite_set.draw_order.iter() {
                    let sprite = sprite_set.map.get(id).unwrap();
                    draw_one_animated_sprite(&transform, sprite);
                }
            }
        }
    }
}

pub fn debug_draw_sprites(world: &mut World) {
    let mut ordered = world
        .query::<&DrawOrder>()
        .iter()
        .map(|(e, order)| (e, order.0))
        .collect::<Vec<_>>();

    ordered.sort_by(|&(_, a), &(_, b)| a.cmp(&b));

    // This needs to be performance tested. If it causes any issues, we can work around having to
    // do all these queries by wrapping all drawables in a `Drawable` enum type and match on that
    // in stead.
    for e in ordered.into_iter().map(|(e, _)| e) {
        if let Ok(transform) = world.get::<Transform>(e) {
            if let Ok(sprite) = world.get::<Sprite>(e) {
                debug_draw_one_sprite(transform.position, &sprite);
            } else if let Ok(sprite_set) = world.get::<SpriteSet>(e) {
                for id in sprite_set.draw_order.iter() {
                    let sprite = sprite_set.map.get(id).unwrap();
                    debug_draw_one_sprite(transform.position, sprite);
                }
            } else if let Ok(sprite) = world.get::<AnimatedSprite>(e) {
                debug_draw_one_animated_sprite(transform.position, &sprite);
            } else if let Ok(sprite_set) = world.get::<AnimatedSpriteSet>(e) {
                for id in sprite_set.draw_order.iter() {
                    let sprite = sprite_set.map.get(id).unwrap();
                    debug_draw_one_animated_sprite(transform.position, sprite);
                }
            }
        }
    }
}
