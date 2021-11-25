// Vertex shader

struct VertexInput {
    [[location(0)]] pos: vec2<f32>;
    [[location(1)]] uv: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] uv: vec2<f32>;
};

[[stage(vertex)]]
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.pos, 0.0, 1.0);
    out.uv = model.uv;
    return out;
}

// Fragment shader
[[block]]
struct Uniforms {
    time: f32;
};


[[group(1), binding(0)]] var<uniform> uniforms: Uniforms;

[[group(0), binding(0)]] var texture: texture_2d<f32>;
[[group(0), binding(1)]] var sampler: sampler;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    var uv: vec2<f32> = in.uv;
    // var offset: vec2<f32> = vec2<f32>(
    //     sin(uniforms.time + uv.y * 1.5) / 20.0,
    //     sin(uniforms.time + uv.x * 15.0) / 30.0
    // );

    let col = textureSample(texture, sampler, uv);
    return col;
}

 