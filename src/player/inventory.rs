use macroquad::prelude::*;

use hecs::{Entity, With, Without, World};

use crate::items::{
    fire_weapon, ItemDepleteBehavior, ItemDropBehavior, Weapon, EFFECT_ANIMATED_SPRITE_ID,
    GROUND_ANIMATION_ID, ITEMS_DRAW_ORDER, SPRITE_ANIMATED_SPRITE_ID,
};
use crate::particles::ParticleEmitter;
use crate::player::{PlayerController, PlayerState, IDLE_ANIMATION_ID, PICKUP_GRACE_TIME};
use crate::{AnimatedSpriteSet, DrawOrder, Item, Owner, PhysicsBody, Transform};

const THROW_FORCE: f32 = 5.0;

#[derive(Default)]
pub struct PlayerInventory {
    pub weapon_mount: Vec2,
    pub weapon: Option<Entity>,
    pub items: Vec<Entity>,
}

impl From<Vec2> for PlayerInventory {
    fn from(weapon_mount: Vec2) -> Self {
        PlayerInventory {
            weapon_mount,
            weapon: None,
            items: Vec::new(),
        }
    }
}

impl PlayerInventory {
    pub fn get_weapon_mount(&self, is_facing_left: bool, is_upside_down: bool) -> Vec2 {
        flip_offset(self.weapon_mount, None, is_facing_left, is_upside_down)
    }
}

pub fn update_player_inventory(world: &mut World) {
    let mut item_colliders = world
        .query::<With<Item, Without<Owner, (&Transform, &PhysicsBody)>>>()
        .iter()
        .map(|(e, (transform, body))| (e, body.as_rect(transform.position)))
        .collect::<Vec<_>>();

    let mut weapon_colliders = world
        .query::<With<Weapon, Without<Owner, (&Transform, &PhysicsBody)>>>()
        .iter()
        .map(|(e, (transform, body))| (e, body.as_rect(transform.position)))
        .collect::<Vec<_>>();

    let mut picked_up = Vec::new();

    let mut to_drop = Vec::new();
    let mut to_fire = Vec::new();
    let mut to_destroy = Vec::new();

    for (entity, (transform, controller, state, inventory, body)) in world
        .query::<(
            &mut Transform,
            &PlayerController,
            &mut PlayerState,
            &mut PlayerInventory,
            &mut PhysicsBody,
        )>()
        .iter()
    {
        if state.is_dead {
            for item_entity in inventory.items.drain(0..) {
                to_drop.push(item_entity);
            }

            if let Some(weapon_entity) = inventory.weapon.take() {
                to_drop.push(weapon_entity);
            }
        } else {
            let player_rect = body.as_rect(transform.position);

            let mut i = 0;
            while i < item_colliders.len() {
                let &(item_entity, rect) = item_colliders.get(i).unwrap();

                if player_rect.overlaps(&rect) {
                    picked_up.push((entity, item_entity));
                    item_colliders.remove(i);

                    inventory.items.push(item_entity);

                    let mut body = world.get_mut::<PhysicsBody>(item_entity).unwrap();
                    body.is_deactivated = true;

                    let mut sprite_set = world.get_mut::<AnimatedSpriteSet>(item_entity).unwrap();
                    sprite_set.set_all(IDLE_ANIMATION_ID, true);

                    continue;
                }

                i += 1;
            }

            if controller.should_pickup {
                if let Some(weapon_entity) = inventory.weapon.take() {
                    to_drop.push(weapon_entity);

                    let velocity = if state.is_facing_left {
                        vec2(-THROW_FORCE, 0.0)
                    } else {
                        vec2(THROW_FORCE, 0.0)
                    };

                    let mut body = world.get_mut::<PhysicsBody>(weapon_entity).unwrap();

                    body.velocity = velocity;
                } else if state.pickup_grace_timer >= PICKUP_GRACE_TIME {
                    for (i, &(weapon_entity, rect)) in weapon_colliders.iter().enumerate() {
                        if player_rect.overlaps(&rect) {
                            picked_up.push((entity, weapon_entity));
                            weapon_colliders.remove(i);

                            inventory.weapon = Some(weapon_entity);
                            state.pickup_grace_timer = 0.0;

                            let mut body = world.get_mut::<PhysicsBody>(weapon_entity).unwrap();
                            body.is_deactivated = true;

                            let mut sprite_set =
                                world.get_mut::<AnimatedSpriteSet>(weapon_entity).unwrap();
                            sprite_set.set_all(IDLE_ANIMATION_ID, true);

                            break;
                        }
                    }
                }
            }

            if let Some(weapon_entity) = inventory.weapon {
                let mut weapon = world.get_mut::<Weapon>(weapon_entity).unwrap();

                weapon.cooldown_timer += get_frame_time();

                let mut weapon_transform = world.get_mut::<Transform>(weapon_entity).unwrap();

                let weapon_mount = transform.position
                    + inventory.get_weapon_mount(state.is_facing_left, state.is_upside_down);

                weapon_transform.position = weapon_mount;

                let mut sprite_set = world.get_mut::<AnimatedSpriteSet>(weapon_entity).unwrap();

                sprite_set.flip_all_x(state.is_facing_left);
                sprite_set.flip_all_y(state.is_upside_down);

                let frame_size = sprite_set
                    .map
                    .get(SPRITE_ANIMATED_SPRITE_ID)
                    .map(|sprite| sprite.size())
                    .unwrap();

                let mount_offset = flip_offset(
                    weapon.mount_offset,
                    frame_size,
                    state.is_facing_left,
                    state.is_upside_down,
                );

                weapon_transform.position += mount_offset;

                if let Ok(mut particle_emitters) =
                    world.get_mut::<Vec<ParticleEmitter>>(weapon_entity)
                {
                    let mut offset = weapon.effect_offset;

                    if state.is_facing_left {
                        offset.x = frame_size.x - offset.x;
                    }

                    if state.is_upside_down {
                        offset.y = frame_size.y - offset.y;
                    }

                    for emitter in particle_emitters.iter_mut() {
                        emitter.offset = offset;
                    }
                }

                let is_depleted = weapon
                    .uses
                    .map(|uses| weapon.use_cnt >= uses)
                    .unwrap_or_default();

                if is_depleted {
                    match weapon.deplete_behavior {
                        ItemDepleteBehavior::Destroy => {
                            to_destroy.push(weapon_entity);
                            inventory.weapon = None;
                        }
                        ItemDepleteBehavior::Drop => {
                            to_drop.push(weapon_entity);
                            inventory.weapon = None;
                        }
                        _ => {}
                    }
                } else if controller.should_attack {
                    to_fire.push((weapon_entity, entity));
                }
            }

            for &entity in inventory.items.iter() {
                let mut item = world.get_mut::<Item>(entity).unwrap();

                item.duration_timer += get_frame_time();

                let position = transform.position;

                match world.get_mut::<AnimatedSpriteSet>(entity) {
                    Ok(mut sprite_set) => {
                        sprite_set.flip_all_x(state.is_facing_left);
                        sprite_set.flip_all_y(state.is_upside_down);

                        let offset = flip_offset(
                            item.mount_offset,
                            sprite_set.size(),
                            state.is_facing_left,
                            state.is_upside_down,
                        );

                        let mut item_transform = world.get_mut::<Transform>(entity).unwrap();
                        item_transform.position = position + offset;
                    }
                    Err(err) => {
                        #[cfg(debug_assertions)]
                        println!("WARNING: {}", err);
                    }
                }
            }
        }
    }

    for (player, item) in picked_up {
        world.insert_one(item, Owner(player)).unwrap();

        let player_draw_order = world.get::<DrawOrder>(player).unwrap();

        let mut draw_order = world.get_mut::<DrawOrder>(item).unwrap();
        draw_order.0 = player_draw_order.0 + 1;
    }

    for entity in to_drop {
        world.remove_one::<Owner>(entity).unwrap();

        let mut should_destroy = false;

        if let Ok(mut weapon) = world.get_mut::<Weapon>(entity) {
            match weapon.drop_behavior {
                ItemDropBehavior::ClearState => {
                    weapon.use_cnt = 0;
                    weapon.cooldown_timer = weapon.cooldown;
                }
                ItemDropBehavior::Destroy => {
                    should_destroy = true;
                }
                _ => {}
            }
        } else if let Ok(mut item) = world.get_mut::<Item>(entity) {
            match item.drop_behavior {
                ItemDropBehavior::ClearState => {
                    item.use_cnt = 0;
                    item.duration_timer = 0.0;
                }
                ItemDropBehavior::Destroy => {
                    should_destroy = true;
                }
                _ => {}
            }
        }

        if should_destroy {
            if let Err(err) = world.despawn(entity) {
                #[cfg(debug_assertions)]
                println!("WARNING: {}", err);
            }
        } else {
            let mut draw_order = world.get_mut::<DrawOrder>(entity).unwrap();
            draw_order.0 = ITEMS_DRAW_ORDER;

            let mut body = world.get_mut::<PhysicsBody>(entity).unwrap();

            body.is_deactivated = false;

            let mut sprite_set = world.get_mut::<AnimatedSpriteSet>(entity).unwrap();

            sprite_set.restart_all();

            let sprite = sprite_set.map.get_mut(SPRITE_ANIMATED_SPRITE_ID).unwrap();

            if sprite
                .animations
                .iter()
                .any(|a| a.id == *GROUND_ANIMATION_ID)
            {
                sprite.set_animation(GROUND_ANIMATION_ID, true);
            } else {
                sprite.set_animation(IDLE_ANIMATION_ID, true);
            }

            if let Some(sprite) = sprite_set.map.get_mut(EFFECT_ANIMATED_SPRITE_ID) {
                sprite.is_deactivated = true;
            }
        }
    }

    for (entity, owner) in to_fire.drain(0..) {
        if let Err(err) = fire_weapon(world, entity, owner) {
            #[cfg(debug_assertions)]
            println!("WARNING: {}", err);
        }
    }

    for entity in to_destroy {
        if let Err(err) = world.despawn(entity) {
            #[cfg(debug_assertions)]
            println!("WARNING: {}", err);
        }
    }
}

const HUD_OFFSET_Y: f32 = 16.0;

const HUD_CONDENSED_USE_COUNT_THRESHOLD: u32 = 12;

const HUD_USE_COUNT_COLOR_FULL: Color = Color {
    r: 0.8,
    g: 0.9,
    b: 1.0,
    a: 1.0,
};

const HUD_USE_COUNT_COLOR_EMPTY: Color = Color {
    r: 0.8,
    g: 0.9,
    b: 1.0,
    a: 0.8,
};

pub fn draw_weapons_hud(world: &mut World) {
    for (_, (transform, inventory)) in world.query::<(&Transform, &PlayerInventory)>().iter() {
        if let Some(weapon_entity) = inventory.weapon {
            let weapon = world.get::<Weapon>(weapon_entity).unwrap();
            if let Some(uses) = weapon.uses {
                let is_destroyed_on_depletion =
                    weapon.deplete_behavior == ItemDepleteBehavior::Destroy;

                if !is_destroyed_on_depletion || uses > 1 {
                    let remaining = uses - weapon.use_cnt;

                    let mut position = transform.position;
                    position.y -= HUD_OFFSET_Y;

                    if uses >= HUD_CONDENSED_USE_COUNT_THRESHOLD {
                        let x = position.x - ((4.0 * uses as f32) / 2.0);

                        for i in 0..uses {
                            draw_rectangle(
                                x + 4.0 * i as f32,
                                position.y - 12.0,
                                2.0,
                                12.0,
                                if i >= remaining {
                                    HUD_USE_COUNT_COLOR_EMPTY
                                } else {
                                    HUD_USE_COUNT_COLOR_FULL
                                },
                            )
                        }
                    } else {
                        let x = position.x - (uses as f32 * 14.0) / 2.0;

                        for i in 0..uses {
                            let x = x + 14.0 * i as f32;

                            if i >= remaining {
                                draw_circle_lines(
                                    x,
                                    position.y - 4.0,
                                    4.0,
                                    2.0,
                                    HUD_USE_COUNT_COLOR_EMPTY,
                                );
                            } else {
                                draw_circle(x, position.y - 4.0, 4.0, HUD_USE_COUNT_COLOR_FULL);
                            };
                        }
                    }
                }
            }
        }
    }
}

pub fn flip_offset<S: Into<Option<Vec2>>>(
    offset: Vec2,
    size: S,
    is_facing_left: bool,
    is_upside_down: bool,
) -> Vec2 {
    let mut corrected = Vec2::ZERO;

    let size = size.into().unwrap_or_default();

    if is_facing_left {
        corrected.x -= offset.x + size.x;
    } else {
        corrected.x = offset.x;
    }

    if is_upside_down {
        corrected.y -= offset.y + size.y;
    } else {
        corrected.y = offset.y;
    }

    corrected
}
