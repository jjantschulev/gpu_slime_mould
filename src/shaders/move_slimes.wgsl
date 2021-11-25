let move_speed : f32 = 0.3;
let pi : f32 = 3.14159265359;
let deposit_amount : vec3<f32> = vec3<f32>(0.01, 0.0, 0.0); 
let sensor_angle = 1.0;
let sensor_distance = 0.0003;
let turn_speed = 15.0;


[[block]]
struct Params {
    width: u32;
    height: u32;
    num_slimes: u32;
    delta_time: f32;
    random: f32;
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

[[group(0), binding(0)]] var<uniform> params: Params;
[[group(0), binding(1)]] var<storage, read_write> slimes: Slimes;

[[group(1), binding(0)]] var input_tex: texture_2d<f32>;
[[group(1), binding(1)]] var output_tex: texture_storage_2d<rgba32float,write>;


fn angle_to_dir(a: f32) -> vec2<f32> {
    return vec2<f32>(cos(a), sin(a));
}

fn sample(pos: vec2<f32>) -> f32 {
    return textureLoad(input_tex, vec2<i32>(pos), 0).x;
}

fn rand(id: u32) -> f32 {
    let co = vec2<f32>(f32(id), params.random);
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

[[stage(compute), workgroup_size(64)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    if (global_ix.x >= params.num_slimes) {
        return;
    }
    
    let slime = slimes.slimes[global_ix.x];
    var next_heading : f32 = slime.heading;

    let left = sample(slime.pos + angle_to_dir(slime.heading - sensor_angle) * sensor_distance);
    let middle = sample(slime.pos + angle_to_dir(slime.heading) * sensor_distance);
    let right = sample(slime.pos + angle_to_dir(slime.heading + sensor_angle) * sensor_distance);
    let sum = left + middle + right;
    let lw = left / sum;
    let mw = middle / sum;
    let rw = right / sum;
    let r = rand(global_ix.x);
    if (r < lw) {
        next_heading = next_heading - turn_speed * params.delta_time;
    } else {
        if (r > lw + mw) {
            next_heading = next_heading + turn_speed * params.delta_time;
        }
    }

    // if (middle > left && middle > right) {

    // } else {if (left > middle && middle > right) {
    //     next_heading = next_heading - turn_speed * params.delta_time;
    // } else {if (right > middle && middle > left) {
    //     next_heading = next_heading + turn_speed * params.delta_time;
    // } else {
    //     if (rand(global_ix.x) < 0.5) {
    //         next_heading = next_heading - turn_speed * params.delta_time;
    //     } else {
    //         next_heading = next_heading + turn_speed * params.delta_time;
    //     }
    // }}}


    var next_pos : vec2<f32> = slime.pos + angle_to_dir(next_heading) * move_speed * params.delta_time;
    if (next_pos.x < 0.0) { next_pos.x = 0.0; next_heading = next_heading - pi; }
    if (next_pos.x > 1.0) { next_pos.x = 1.0; next_heading = next_heading - pi; }
    if (next_pos.y < 0.0) { next_pos.y = 0.0; next_heading = -next_heading; }
    if (next_pos.y > 1.0) { next_pos.y = 1.0; next_heading = -next_heading; }
    slimes.slimes[global_ix.x].pos = next_pos;
    slimes.slimes[global_ix.x].heading = next_heading;

    // Store final slime position in the texture;
    let slime_coord: vec2<i32> = vec2<i32>(slime.pos * vec2<f32>(f32(params.width), f32(params.height)));
    let slime_color: vec4<f32> = vec4<f32>(deposit_amount, 1.0) * params.delta_time;

    let color = textureLoad(input_tex, slime_coord, 0);
    
    textureStore(output_tex, slime_coord, color + slime_color);
}

