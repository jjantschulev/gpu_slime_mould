let blur_kernel_size : i32 = 1;
let dissipate_amt: f32 = 0.95;

[[block]]
struct Params {
    delta_time: f32;
};

[[block]]
struct StaticParams {
    width: u32;
    height: u32;
    num_slimes: u32;
};

[[block]] struct World {
    values: array<f32>;
};

[[group(0), binding(0)]] var<uniform> params: Params;
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


[[stage(compute), workgroup_size(8, 8, 1)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    let tex_index = vec2<i32>(global_ix.xy);
    // let current_val = load(tex_index);

    var avg_val : f32 = 0.0;

    var x : i32 = -blur_kernel_size;
    var y : i32 = -blur_kernel_size;
    var num_samples : i32 = 0;
    loop {
        if (x > blur_kernel_size) { break; }
        if (y > blur_kernel_size) { 
            x = x + 1;
            y = -blur_kernel_size;
        } else {
            let point = tex_index + vec2<i32>(y, x);
            if (point.x > 0 && point.y > 0 && point.x < i32(static_params.width) && point.y < i32(static_params.height)) {
                avg_val = avg_val + load(point);
                num_samples = num_samples + 1;
            }
            y = y + 1;
        }
    }

    avg_val = avg_val / f32(num_samples);
    let next_val = avg_val * dissipate_amt;

    store(tex_index, clamp(next_val, 0.0, 2.0));
}
