#version 330 core

in vec2 f_uvs;

out vec4 frag_color;

uniform sampler2D image_texture;
uniform bool monochrome = false;

void main() {
    vec2 texel_size = 1.0 / textureSize(image_texture, 0);

    //Check which effect is enabled and apply it
    if (monochrome) {
        vec3 intermediate = texture(image_texture, f_uvs).xyz;
        float average = (intermediate.r + intermediate.g + intermediate.b) / 3.0;
        frag_color = vec4(average, average, average, 1.0);
    } else {
        vec3 intermediate = texture(image_texture, f_uvs).xyz;
        frag_color = vec4(intermediate, 1.0);
    }
}