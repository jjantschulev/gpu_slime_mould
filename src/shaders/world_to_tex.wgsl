[[block]] struct World {
    values: array<f32>;
};

[[block]]
struct StaticParams {
    width: u32;
    height: u32;
    num_slimes: u32;
};


[[block]]
struct Params {
    delta_time: f32;
    random: f32;
};

[[group(1), binding(0)]] var<uniform> static_params: StaticParams;
[[group(2), binding(0)]] var<uniform> params: Params;
[[group(0), binding(0)]] var<storage, read> input_buf: World;
[[group(0), binding(1)]] var output_tex: texture_storage_2d<rgba8unorm, write>;

fn load(index: vec2<i32>) -> f32 {
    return input_buf.values[index.x + index.y * i32(static_params.width)];
}

fn rand(seed: f32) -> f32 {
    let co = vec2<f32>(seed, params.random);
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// fn random(seed: f32) -> f32 {
//     let p = vec2<f32>(seed, params.random);
//     let k1 = vec2<f32>(
//         23.14069263277926, // e^pi (Gelfond's constant)
//         2.665144142690225  // 2^sqrt(2) (Gelfondâ€“Schneider constant)
//     );
//     return fract( cos( dot(p,k1) ) * 123456.6789 * params.random );
// }

// // A single iteration of Bob Jenkins' One-At-A-Time hashing algorithm.
// fn hash(x: u32) -> u32 {
//     var xmod : u32 = x;
//     xmod = xmod + ( xmod << 10u );
//     xmod = xmod ^ ( xmod >>  6u );
//     xmod = xmod + ( xmod <<  3u );
//     xmod = xmod ^ ( xmod >> 11u );
//     xmod = xmod + ( xmod << 15u );
//     return xmod;
// }
// fn hash( v : vec2<u32> ) { return hash( v.x ^ hash(v.y)                         ); }
// fn hash( v : vec3<u32> ) { return hash( v.x ^ hash(v.y) ^ hash(v.z)             ); }
// fn hash( v : vec4<u32> ) { return hash( v.x ^ hash(v.y) ^ hash(v.z) ^ hash(v.w) ); }
// // Construct a float with half-open range [0:1] using low 23 bits.
// // All zeroes yields 0.0, all ones yields the next smallest representable value below 1.0.
// fn floatConstruct( m: u32 ) -> f32 {
//     let ieeeMantissa : u32 = 0x007FFFFFu; // binary32 mantissa bitmask
//     let ieeeOne : u32      = 0x3F800000u; // 1.0 in IEEE binary32
//     var mmod : u32 = m;
//     mmod = mmod & ieeeMantissa;                     // Keep only mantissa bits (fractional part)
//     mmod = mmod | ieeeOne;                          // Add fractional part to 1.0

//     let f = uintBitsToFloat( m );          // Range [1:2]
//     return f - 1.0;                        // Range [0:1]
// }
// fn random( x : f32         ) -> fn { return floatConstruct(hash(floatBitsToUint(x))); }
// fn random( v : vec2<f32>   ) -> fn { return floatConstruct(hash(floatBitsToUint(v))); }
// fn random( v : vec3<f32>   ) -> fn { return floatConstruct(hash(floatBitsToUint(v))); }
// fn random( v : vec4<f32>   ) -> fn { return floatConstruct(hash(floatBitsToUint(v))); }





[[stage(compute), workgroup_size(8, 8, 1)]]
fn main([[builtin(global_invocation_id)]] global_ix: vec3<u32>) {
    let tex_index = vec2<i32>(global_ix.xy);
    let val = load(tex_index);
    // let val = rand(f32(global_ix.x + global_ix.y * static_params.width) / f32(static_params.width * static_params.height));
    let frag_coord = vec2<f32>(global_ix.xy) / vec2<f32>(f32(static_params.width), f32(static_params.width));
    let color = vec4<f32>(frag_coord, 0.3, 1.0) * val;// * pow(val, 1.0);
    // let color = vec4<f32>(val, val, val, 1.0);
    textureStore(output_tex, tex_index, vec4<f32>(color.rgb, 1.0));
}
