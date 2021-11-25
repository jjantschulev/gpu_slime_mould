let blur_kernel_size : i32 = 1;
let diffuse_amt: f32 = 1.0;
let dissipate_amt: f32 = 0.005;

[[block]]
struct Params {
    width: u32;
    height: u32;
    delta_time: f32;
};

[[group(0), binding(0)]] var<uniform> params: Params;

[[group(1), binding(0)]] var input_tex: texture_2d<f32>;
[[group(1), binding(1)]] var output_tex: texture_storage_2d<rgba32float,write>;

[[stage(compute), workgroup_size(16, 16)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    let in_color = textureLoad(input_tex, vec2<i32>(global_ix.xy), 0);

    var avg_color : vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    var x : i32 = -blur_kernel_size;
    var y : i32 = -blur_kernel_size;
    var num_samples : i32 = 0;
    loop {
        if (x > blur_kernel_size) { break; }
        if (y > blur_kernel_size) { 
            x = x + 1;
            y = -blur_kernel_size;
        } else {
            let point = vec2<i32>(global_ix.xy) + vec2<i32>(y, x);
            if (point.x > 0 && point.y > 0 && point.x < i32(params.width) && point.y < i32(params.height)) {
                avg_color = avg_color + textureLoad(input_tex, point, 0);
                num_samples = num_samples + 1;
            }
            y = y + 1;
        }
    }

    avg_color = avg_color / f32(num_samples);
    let difference = avg_color - in_color;

    var color : vec4<f32> = avg_color + difference * params.delta_time * diffuse_amt;

    color = vec4<f32>(color.rgb - dissipate_amt * params.delta_time, 1.0);

    textureStore(output_tex, vec2<i32>(global_ix.xy), color);
}
