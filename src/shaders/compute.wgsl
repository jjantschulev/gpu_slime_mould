// Struct Definitions
struct Slime {
    x: f32;
    y: f32;
    heading: f32;
};
[[block]] struct Slimes {
    slimes: array<Slime>;
};

[[block]] struct OutTex {
    pixels: array<f32>;
};


// Shared Data Definitions
[[group(0), binding(0)]] var<storage, read_write> slimes : Slimes;
[[group(0), binding(1)]] var<storage, read_write> out_tex : OutTex;


// Functions
fn angle_to_vec(angle: f32) -> vec2<f32> {
    return vec2<f32>(cos(angle), sin(angle));
}

[[stage(compute), workgroup_size(512)]]
fn compute_main([[builtin(global_invocation_id)]] global_id : vec3<u32>) {
    let i = global_id.x;
    var slime: Slime = slimes.slimes[i];
    let slime_pos = vec2<f32>(slime.x, slime.y);
    let dir = angle_to_vec(slime.heading);
    var next : vec2<f32> = slime_pos + (dir * 0.001);
    if (next.x < 0.0) {
       next.x = 1.0;
    }
    if (next.x >= 1.0) {
       next.x = 0.0;
    }
    if (next.y < 0.0) {
       next.y = 1.0;
    }
    if (next.y >= 1.0) {
       next.y = 0.0;
    }
    slime.x = next.x;
    slime.y = next.y;
    let ix = u32(slime.x * 1024.0);
    let iy = u32(slime.y * 1024.0);
    slimes.slimes[i] = slime;
    let pixel_index = ix + (iy * u32(1024 * 4));
    out_tex.pixels[pixel_index + u32(0)] = 1.0;
    out_tex.pixels[pixel_index + u32(1)] = 1.0;
    out_tex.pixels[pixel_index + u32(2)] = 1.0;
    out_tex.pixels[pixel_index + u32(3)] = 1.0;
}