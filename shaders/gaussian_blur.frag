#version 330 core

in vec2 f_uvs;

out vec4 frag_color;

uniform sampler2D image_texture;
uniform bool horizontal;

const int TAP = 5; //Must be odd
const int BOUND = TAP / 2;

//The values for these weights are merely contants generated from a python script
//const float GAUSSIAN_WEIGHTS[TAP / 2 + 1] = float[](0.1913811563169165, 0.17224304068522484, 0.1252676659528908, 0.07307280513918629, 0.03372591006423983);
const float GAUSSIAN_WEIGHTS[TAP / 2 + 1] = float[](0.23273657289002558, 0.21483375959079284, 0.16879795396419436);
const float MIPMAP_LEVEL = 3;

void main() {
    vec2 texel_size = 1.0 / textureSize(image_texture, 0);
    vec2 unit_uv_offset = horizontal ? vec2(1, 0) : vec2(0, 1);
    vec3 result = vec3(0.0);
    for (int i = -BOUND; i <= BOUND; i++) {
        float weight = GAUSSIAN_WEIGHTS[abs(i)];
        vec3 sample = textureLod(image_texture, f_uvs + i * unit_uv_offset * texel_size, MIPMAP_LEVEL).xyz;
        result += sample * weight;
    }
    frag_color = vec4(result, 1.0);
}