struct MoveParams {
    move_speed: f32;
    turn_speed: f32;
    sensor_distance: f32;
    sensor_angle: f32;
    deposit_amount: f32; 
};

let pi: f32 = 3.14159265359;

let move_params_arr: array<MoveParams, 4> = array<MoveParams, 4>(
    MoveParams (
        5.0, // speed
        0.4, // turn speed
        6.0, // sense distance
        0.5, // sense angle
        0.1, // deposit
    ),
    MoveParams (
        5.0, // speed
        0.4, // turn speed
        6.0, // sense distance
        0.5, // sense angle
        0.1, // deposit
    ),
    MoveParams (
        2.1, // speed
        0.3, // turn speed
        45.0, // sense distance
        0.82, // sense angle
        0.2, // deposit
    ),
    MoveParams (
        0.2, // speed
        0.4, // turn speed
        5.0, // sense distance
        0.5, // sense angle
        0.2, // deposit
    ),
);


[[block]]
struct Params {
    delta_time: f32;
    random: f32;
    move_to_center: u32;
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
    values: [[stride(16)]] array<vec4<f32>>;
};

[[group(0), binding(0)]] var<uniform> params: Params;
[[group(0), binding(1)]] var<storage, read_write> slimes: Slimes;
[[group(2), binding(0)]] var<uniform> static_params: StaticParams;

[[group(1), binding(0)]] var<storage, read> input_buf: World;
[[group(1), binding(1)]] var<storage, read_write> output_buf: World;

fn load(index: vec2<i32>) -> vec4<f32> {
    if (index.x >= 0 && index.y >= 0 && index.x < i32(static_params.width) && index.y < i32(static_params.height)) {
        return input_buf.values[index.x + index.y * i32(static_params.width)];
    } else {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}

fn store(index: vec2<i32>, value: vec4<f32>) -> void {
    if (index.x >= 0 && index.y >= 0 && index.x < i32(static_params.width) && index.y < i32(static_params.height)) {
        output_buf.values[index.x + index.y * i32(static_params.width)] = value;
    }
}

fn angle_to_dir(a: f32) -> vec2<f32> {
    return vec2<f32>(cos(a), sin(a));
}

fn rand(id: u32) -> f32 {
    let co = vec2<f32>(f32(id), params.random);
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}


fn sample_filter(pos: vec2<f32>) -> f32 {
    let radius = f32(static_params.height) * 0.30;
    let center = vec2<f32>(f32(static_params.width)/2.0, f32(static_params.height)/2.0);
    let dist = distance(pos, center);
    if (dist > radius) {
        return 0.0;
    } else {
        return 1.0;
    }
}

fn sample(pos: vec2<f32>, species: u32) -> f32 {
    let val = load(vec2<i32>(pos));
    let sample = sample_filter(pos);
    let center = vec2<f32>(f32(static_params.width)/2.0, f32(static_params.height)/2.0);
    let dist_home = 0.6 - distance(pos, center) * 0.0005;
    if (species == 0u) {
        if(sample == 0.0) {
            return (dist_home) * val.x;
        } else {
            return sample * (val.x / val.y);
        }
    }
    if (species == 1u) {
        if(sample == 0.0) {
            return dist_home - val.x;
        } else {
            return sample * (val.y / val.x);
        }
        // return val.x * val.y;
    }
    if (species == 2u) {
        return val.z;
    }
    if (species == 3u) {
        return val.w;
    }
    return 0.0;
}


[[stage(compute), workgroup_size(64, 1, 1)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    if (global_ix.x >= static_params.num_slimes) {
        return;
    }


    
    let slime = slimes.slimes[global_ix.x];

    var move_params : MoveParams = move_params_arr[0];
    if (slime.species == 1u) {
        move_params = move_params_arr[1];
    }
    if (slime.species == 2u) {
        move_params = move_params_arr[2];
    }
    if (slime.species == 3u) {
        move_params = move_params_arr[3];
    }
    var next_heading : f32 = slime.heading;

    let left_sample_pos = slime.pos + angle_to_dir(slime.heading - move_params.sensor_angle) * move_params.sensor_distance;
    let middle_sample_pos = slime.pos + angle_to_dir(slime.heading) * move_params.sensor_distance;
    let right_sample_pos = slime.pos + angle_to_dir(slime.heading + move_params.sensor_angle) * move_params.sensor_distance;

    // let center = vec2<f32>(f32(static_params.width)/2.0, f32(static_params.height)/2.0);
    // let dlc = distance(center, left_sample_pos);
    // let dmc = distance(center, middle_sample_pos);
    // let drc = distance(center, right_sample_pos);
    // let dmax = max(dlc, max(dmc, drc));
    // var weight : f32 = 1.7;
    // if (params.move_to_center == u32(0)) {
    //     weight = 0.0;
    // }
    // let dlcn = (1.0 - (dlc / dmax)) * weight;
    // let dmcn = (1.0 - (dmc / dmax)) * weight;
    // let drcn = (1.0 - (drc / dmax)) * weight;

    let left = sample(left_sample_pos, slime.species);// + dlcn;
    let middle = sample(middle_sample_pos, slime.species);// + dmcn;
    let right = sample(right_sample_pos, slime.species);// + drcn;

    if (middle > left && middle > right) {

    } else {if (left > middle && middle > right) {
        next_heading = next_heading - move_params.turn_speed;
    } else {if (right > middle && middle > left) {
        next_heading = next_heading + move_params.turn_speed;
    } else {
        if (rand(global_ix.x) < 0.5) {
            next_heading = next_heading - move_params.turn_speed;
        } else {
            next_heading = next_heading + move_params.turn_speed;
        }
    }}}

    var next_pos : vec2<f32> = slime.pos + angle_to_dir(next_heading) * move_params.move_speed;
    if (next_pos.x < 0.0) { next_pos.x = 0.0; next_heading = next_heading - pi; }
    if (next_pos.x > f32(static_params.width)) { next_pos.x = f32(static_params.width); next_heading = next_heading - pi; }
    if (next_pos.y < 0.0) { next_pos.y = 0.0; next_heading = -next_heading; }
    if (next_pos.y > f32(static_params.height)) { next_pos.y = f32(static_params.height); next_heading = -next_heading; }
    slimes.slimes[global_ix.x].pos = next_pos;
    slimes.slimes[global_ix.x].heading = next_heading;

    // Store final slime position in the texture;
    let slime_coord = vec2<i32>(next_pos);

    var existing : vec4<f32> = load(slime_coord);
    if (slime.species == 0u) {
        existing.x = existing.x + move_params.deposit_amount;
    }
    if (slime.species == 1u) {
        existing.y = existing.y + move_params.deposit_amount;
    }
    if (slime.species == 2u) {
        existing.z = existing.z + move_params.deposit_amount;
    }
    if (slime.species == 3u) {
        existing.w = existing.w + move_params.deposit_amount;
    }

    store(slime_coord, existing);
}

