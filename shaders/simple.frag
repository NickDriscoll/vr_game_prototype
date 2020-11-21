#version 330 core

in vec3 f_normal;
in vec4 shadow_space_pos;
in vec2 f_uvs;

out vec4 frag_color;

//Material maps
uniform sampler2D albedo_map;
uniform sampler2D normal_map;
uniform sampler2D roughness_map;

//Shadow map
uniform sampler2D shadow_map;

uniform vec4 sun_direction;
uniform vec4 view_direction;
uniform float uv_scale = 1.0;

const float AMBIENT = 0.1;

void main() {
    vec2 scaled_uvs = f_uvs * uv_scale;

    vec3 albedo = texture(albedo_map, scaled_uvs).xyz;
    vec3 tangent_normal = texture(normal_map, scaled_uvs).xyz * 2.0 - 1.0;
    vec3 normal = normalize(f_normal);

    //Determine if the fragment is shadowed
    float shadow = 0.0; 
    vec4 adj_shadow_space_pos = shadow_space_pos * 0.5 + 0.5;
    vec2 texel_size = 1.0 / textureSize(shadow_map, 0);
    
    if (adj_shadow_space_pos.z > 1.0 || adj_shadow_space_pos.x < 0.0 || adj_shadow_space_pos.x > 1.0 || adj_shadow_space_pos.y < 0.0 || adj_shadow_space_pos.y > 1.0) {
        shadow = 0.0;
    }
    else {
        //Do PCF
        //Average the nxn block of shadow texels centered at this pixel
        int n = 3;
        int bound = n - 2;
        for (int x = -bound; x <= bound; x++) {
            for (int y = -bound; y <= bound; y++) {
                float sampled_depth = texture(shadow_map, adj_shadow_space_pos.xy + vec2(x, y) * texel_size).r;
                shadow += sampled_depth < adj_shadow_space_pos.z ? 1.0 : 0.0;
            }
        }
        shadow /= 9.0;
    }

    float diffuse = max(0.0, dot(vec3(sun_direction), normal));

    float roughness = texture(roughness_map, scaled_uvs).x;
    vec4 halfway = normalize(view_direction + sun_direction);
    float specular_angle = max(0.0, dot(vec3(halfway), normal));
    float specular_coefficient = 100 / (500 * roughness + 0.01);
    float specular = pow(specular_angle, specular_coefficient);

    vec3 final_color = ((specular + diffuse) * (1.0 - shadow) + AMBIENT) * albedo;
    frag_color = vec4(final_color, 1.0);
    //frag_color = vec4(normal / 2.0 + 0.5, 1.0);
}