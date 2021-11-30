let pi: f32 = 3.14159265359;
let move_speed: f32 = 2.5;
let deposit_amount: f32 = 0.1; 
let sensor_angle: f32 = 0.8;
let sensor_distance: f32 = 10.0;
let turn_speed: f32 = 0.8;


[[block]]
struct Params {
    delta_time: f32;
    random: f32;
};

[[block]]
struct StaticParams {
    width: u32;
    height: u32;
    num_slimes: u32;
};

struct Slime {
    pos: vec2<f32>;
    heading: f32;
    species: u32;
};

[[block]]
struct Slimes {
    slimes: [[stride(16)]] array<Slime>;
};

[[block]] struct World {
    values: [[stride(4)]] array<f32>;
};

[[group(0), binding(0)]] var<uniform> params: Params;
[[group(0), binding(1)]] var<storage, read_write> slimes: Slimes;
[[group(2), binding(0)]] var<uniform> static_params: StaticParams;

[[group(1), binding(0)]] var<storage, read> input_buf: World;
[[group(1), binding(1)]] var<storage, read_write> output_buf: World;

fn load(index: vec2<i32>) -> f32 {
    if (index.x >= 0 && index.y >= 0 && index.x < i32(static_params.width) && index.y < i32(static_params.height)) {
        return input_buf.values[index.x + index.y * i32(static_params.width)];
    } else {
        return 0.0;
    }
}

fn store(index: vec2<i32>, value: f32) -> void {
    if (index.x >= 0 && index.y >= 0 && index.x < i32(static_params.width) && index.y < i32(static_params.height)) {
        output_buf.values[index.x + index.y * i32(static_params.width)] = value;
    }
}

fn angle_to_dir(a: f32) -> vec2<f32> {
    return vec2<f32>(cos(a), sin(a));
}

fn sample(pos: vec2<f32>) -> f32 {
    return load(vec2<i32>(pos));
}

fn rand(id: u32) -> f32 {
    let co = vec2<f32>(f32(id), params.random);
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

[[stage(compute), workgroup_size(64, 1, 1)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    if (global_ix.x >= static_params.num_slimes) {
        return;
    }
    
    let slime = slimes.slimes[global_ix.x];
    var next_heading : f32 = slime.heading;

    let left = sample(slime.pos + angle_to_dir(slime.heading - sensor_angle) * sensor_distance);
    let middle = sample(slime.pos + angle_to_dir(slime.heading) * sensor_distance);
    let right = sample(slime.pos + angle_to_dir(slime.heading + sensor_angle) * sensor_distance);
    // let sum = left + middle + right;
    // let lw = left / sum;
    // let mw = middle / sum;
    // let rw = right / sum;
    // let r = rand(global_ix.x);
    // if (r < lw) {
    //     next_heading = next_heading - turn_speed;
    // } else {
    //     if (r > lw + mw) {
    //         next_heading = next_heading + turn_speed;
    //     }
    // }

    if (middle > left && middle > right) {

    } else {if (left > middle && middle > right) {
        next_heading = next_heading - turn_speed;
    } else {if (right > middle && middle > left) {
        next_heading = next_heading + turn_speed;
    } else {
        if (rand(global_ix.x) < 0.5) {
            next_heading = next_heading - turn_speed;
        } else {
            next_heading = next_heading + turn_speed;
        }
    }}}

    var next_pos : vec2<f32> = slime.pos + angle_to_dir(next_heading) * move_speed;
    if (next_pos.x < 0.0) { next_pos.x = 0.0; next_heading = next_heading - pi; }
    if (next_pos.x > f32(static_params.width)) { next_pos.x = f32(static_params.width); next_heading = next_heading - pi; }
    if (next_pos.y < 0.0) { next_pos.y = 0.0; next_heading = -next_heading; }
    if (next_pos.y > f32(static_params.height)) { next_pos.y = f32(static_params.height); next_heading = -next_heading; }
    slimes.slimes[global_ix.x].pos = next_pos;
    slimes.slimes[global_ix.x].heading = next_heading;

    // Store final slime position in the texture;
    let slime_coord = vec2<i32>(next_pos);

    let existing = load(slime_coord);    
    store(slime_coord, existing + deposit_amount);
}

