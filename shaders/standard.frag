#version 430 core

const float LOD_DIST0 = 20.0;
const float LOD_DIST1 = 40.0;
const float LOD_DIST2 = 170.0;
const float LOD_DIST3 = 240.0;
const vec3 LOD_COLOR0 = vec3(1.0, 0.0, 0.0);
const vec3 LOD_COLOR1 = vec3(1.0, 0.57, 0.0);
const vec3 LOD_COLOR2 = vec3(0.0, 1.0, 0.0);
const vec3 LOD_COLOR3 = vec3(1.0, 0.0, 1.0);
const vec3 LOD_COLOR4 = vec3(0.0, 0.0, 1.0);
const int SHADOW_CASCADES = 5;
const float SHADOW_CASCADES_RECIPROCAL = 1.0 / SHADOW_CASCADES;

in vec3 tangent_sun_direction;
in vec3 tangent_view_position;
in vec3 world_view_position;
in vec3 f_tan_pos;
in vec3 surface_normal;
in vec4 shadow_space_pos[SHADOW_CASCADES];
in vec3 f_world_pos;
in vec2 f_uvs;
in float clip_space_z;
in float f_highlighted;
in mat3 tangent_from_world;

out vec4 frag_color;

//Material textures
uniform sampler2D albedo_sampler;
uniform sampler2D normal_sampler;
uniform sampler2D roughness_sampler;

uniform sampler2D shadow_map;                       //Shadow map texture

uniform vec3 view_position;                         //World space position of the camera
uniform bool complex_normals = false;               //Flag that controls whether or not we sample the normal from the normal map

uniform samplerCube skybox_sampler;

//Debug visualization flags
uniform bool visualize_albedo = false;
uniform bool visualize_normals = false;
uniform bool visualize_shadowed = false;
uniform bool visualize_cascade_zone = false;
uniform bool toon_shading = false;

uniform vec3 sun_color = vec3(1.0, 1.0, 1.0);
uniform float ambient_strength = 0.0;
uniform float shininess_lower_bound = 8.0;
uniform float shininess_upper_bound = 128.0;
uniform float shadow_intensity = 0.1;
uniform float cascade_distances[SHADOW_CASCADES];
uniform float current_time;

/*
layout (std140, binding = 0) uniform SceneData {
    
};
*/

const int MAX_POINT_LIGHTS = 8;
layout (std140, binding = 1) uniform PointLights {
    vec3 positions[MAX_POINT_LIGHTS];
    vec3 colors[MAX_POINT_LIGHTS];

    vec4 radii[MAX_POINT_LIGHTS / 4];   //These are individual floats packed as vec4's to save memory given the std140 layout
} point_lights;
uniform int point_lights_count = 0;

vec3 tangent_space_normal() {
    //Compute this frag's tangent space normal
    vec3 tangent_space_normal = vec3(0.0, 0.0, 1.0);
    if (complex_normals) {
        vec3 sampled_normal = texture(normal_sampler, f_uvs).xyz;
        tangent_space_normal = normalize(sampled_normal * 2.0 - 1.0);
    }
    return tangent_space_normal;
}

vec4 simple_diffuse(vec3 color, float diffuse, float ambient) {
    return vec4((diffuse + ambient) * color, 1.0);
}

float determine_shadowed(vec3 f_shadow_pos, vec3 tan_normal, int cascade) {
    float bias = 0.00125;
    //float bias = 0.0025 * (1.0 - max(0.0, dot(tan_normal, tangent_sun_direction)));
    vec2 sample_uv = f_shadow_pos.xy;
    sample_uv.x = sample_uv.x * SHADOW_CASCADES_RECIPROCAL;
    sample_uv.x += cascade * SHADOW_CASCADES_RECIPROCAL;
    float sampled_depth = texture(shadow_map, sample_uv).r;
    return sampled_depth + bias < f_shadow_pos.z ? 1.0 : 0.0;
}

float lambertian_diffuse(vec3 light_direction, vec3 normal) { return max(0.0, dot(light_direction, normal)); }
float toon_diffuse(vec3 light_direction, vec3 normal) { return smoothstep(0.35, 0.45, lambertian_diffuse(light_direction, normal)); }

float blinn_phong_specular(vec3 view_direction, vec3 light_direction, vec3 normal, float shininess) {
    vec3 halfway = normalize(view_direction + light_direction);
    float spec_angle = max(0.0, dot(halfway, normal));
    return pow(spec_angle, shininess);
}
float toon_specular(vec3 view_direction, vec3 light_direction, vec3 normal, float shininess) { return smoothstep(0.8, 0.9, blinn_phong_specular(view_direction, light_direction, normal, shininess)); }

//Returns the shadow space position as a vec4 where alpha is the shadow cascade index
vec4 determine_shadow_cascade() {
    //Determine which cascade the fragment is in
    vec3 adj_shadow_space_pos;
    int shadow_cascade = -1;
    for (int i = 0; i < SHADOW_CASCADES; i++) {
        if (clip_space_z < cascade_distances[i]) {
            adj_shadow_space_pos = vec3(shadow_space_pos[i] * 0.5 + 0.5);
            if (!(
                adj_shadow_space_pos.z < 0.0 ||
                adj_shadow_space_pos.z > 1.0 ||
                adj_shadow_space_pos.x < 0.0 ||
                adj_shadow_space_pos.x > 1.0 ||
                adj_shadow_space_pos.y < 0.0 ||
                adj_shadow_space_pos.y > 1.0
            )) {
                shadow_cascade = i;
            }
            break;
        }
    }
    return vec4(adj_shadow_space_pos, shadow_cascade);
}

float cascaded_shadow_factor(vec3 adj_shadow_space_pos, int shadow_cascade, vec3 normal) {
    //Compute how shadowed if we are potentially shadowed
    float shadow = 0.0;
    if (shadow_cascade > -1) {
        if (true) {
            //Do PCF
            //Average the 3x3 block of shadow texels centered at this pixel
            int bound = 1;
            vec2 texel_size = 1.0 / textureSize(shadow_map, 0);
            for (int x = -bound; x <= bound; x++) {
                for (int y = -bound; y <= bound; y++) {
                    shadow += determine_shadowed(vec3(adj_shadow_space_pos.xy + vec2(x, y) * texel_size, adj_shadow_space_pos.z), normal, shadow_cascade);
                }
            }
            float s = 2.0 * bound + 1.0;
            shadow /= s * s; //Total number of texels sampled: (2*bound + 1)^2
        } else {
            shadow = determine_shadowed(adj_shadow_space_pos.xyz, normal, shadow_cascade);
        }
    }
    shadow *= shadow_intensity;
    return 1.0 - shadow;
}

//Returns shadow factor in x and cascade index in y
vec2 compute_sun_shadowing(vec3 normal) {
    //Compute sun shadow factor
    vec4 cascade_res = determine_shadow_cascade();
    vec3 adj_shadow_space_pos = cascade_res.xyz;
    int shadow_cascade = int(cascade_res.a);
    float shadow_factor = cascaded_shadow_factor(adj_shadow_space_pos, shadow_cascade, normal);
    return vec2(shadow_factor, float(shadow_cascade));
}

float distance_falloff(float light_radius, float dist) {
    return light_radius * light_radius / (dist * dist + 0.01);
}

void visualize_shadow_cascade(float diffuse, float ambient, float shadow_factor, int shadow_cascade) {    
    if (shadow_cascade == 0) {
        frag_color = simple_diffuse(LOD_COLOR0, diffuse * shadow_factor, ambient);
    } else if (shadow_cascade == 1) {
        frag_color = simple_diffuse(LOD_COLOR1, diffuse * shadow_factor, ambient);
    } else if (shadow_cascade == 2) {
        frag_color = simple_diffuse(LOD_COLOR2, diffuse * shadow_factor, ambient);
    } else if (shadow_cascade == 3) {
        frag_color = simple_diffuse(LOD_COLOR3, diffuse * shadow_factor, ambient);
    }
}

//Returned alpha channel stores distance falloff
vec4 point_light_info(int i) {    
    //Unpack the radius
    int r_idx = i / 4;
    float rs[4] = {point_lights.radii[r_idx].x, point_lights.radii[r_idx].y, point_lights.radii[r_idx].z, point_lights.radii[r_idx].w};
    float radius = rs[i % 4];

    vec3 light_color = point_lights.colors[i];  //Light color

    //Get distance and direction to light in tangent space
    vec3 tangent_light_direction = tangent_from_world * (point_lights.positions[i] - f_world_pos);
    float dist = length(tangent_light_direction);
    tangent_light_direction = normalize(tangent_light_direction);

    return vec4(tangent_light_direction, distance_falloff(radius, dist));
}

vec4 rim_lighting(vec3 view_direction, vec3 normal) {
    //Optionally add rim-lighting
    vec4 rim_lighting = vec4(0.0);
    if (f_highlighted != 0.0) {
        float likeness = 1.0 - max(0.0, dot(view_direction, normal));
        float factor = smoothstep(0.5, 1.0, likeness);
        rim_lighting = vec4(factor * vec3(cos(5.0 * current_time) * 0.5 + 0.5, sin(6.0 * current_time) * 0.5 + 0.5, sin(8.0 * current_time) * 0.5 + 0.5), 1.0);
    }
    return rim_lighting;
}

void shade_blinn_phong() {
    //Sample albedo texture
    vec4 albedo_sample = texture(albedo_sampler, f_uvs);
    if (visualize_albedo) {
        frag_color = albedo_sample;
        return;
    }

    //Compute this frag's tangent space normal
    vec3 tangent_space_normal = tangent_space_normal();
    
    //Early exit if we're visualizing normals
    if (visualize_normals) {
        frag_color = vec4(tangent_space_normal * 0.5 + 0.5, 1.0);
        return;
    }

    //Compute diffuse lighting
    float sun_diffuse = lambertian_diffuse(tangent_sun_direction, tangent_space_normal);

    //Compute sun shadow factor
    vec2 sun_shadowing = compute_sun_shadowing(tangent_space_normal);
    float shadow_factor = sun_shadowing.x;
    int shadow_cascade = int(sun_shadowing.y);

    if (visualize_cascade_zone) {
        visualize_shadow_cascade(sun_diffuse, ambient_strength, shadow_factor, shadow_cascade);
        return;
    }

    //Early exit for shadow visualization
    if (visualize_shadowed) {
        frag_color = vec4(vec3(shadow_factor), 1.0);
        return;
    }

    //Roughness is a [0, 1] value that gets mapped to [shininess_upper_bound, shininess_lower_bound]
    float roughness = texture(roughness_sampler, f_uvs).x;
    float f_shininess = (1.0 - roughness) * (shininess_upper_bound - shininess_lower_bound) + shininess_lower_bound;

    //Compute specular light w/ blinn-phong
    vec3 tangent_view_direction = normalize(tangent_view_position - f_tan_pos);
    float sun_specular = blinn_phong_specular(tangent_view_direction, tangent_sun_direction, tangent_space_normal, f_shininess);

    //Compute lighting from point lights
    vec3 point_lights_contribution = vec3(0.0);
    for (int i = 0; i < point_lights_count; i++) {
        vec4 direction_and_falloff = point_light_info(i);
        vec3 tangent_light_direction = direction_and_falloff.xyz;
        float falloff = direction_and_falloff.a;

        float diffuse = lambertian_diffuse(tangent_light_direction, tangent_space_normal);
        float specular = blinn_phong_specular(tangent_view_direction, tangent_light_direction, tangent_space_normal, f_shininess);

        vec3 light_color = point_lights.colors[i];  //Light color
        point_lights_contribution += light_color * (diffuse + specular) * falloff;
    }

    //Get some light from the skybox
    vec3 sky_contribution = vec3(0.0);
    {
        vec3 f_surface_normal = normalize(surface_normal);
        vec3 world_view_direction = normalize(f_world_pos - world_view_position);
        vec3 sky_sample_vector = reflect(world_view_direction, f_surface_normal);
        sky_sample_vector = sky_sample_vector.xzy * vec3(1.0, 1.0, -1.0);
        float sky_percentage = mix(0.001, 0.25, f_shininess / 128.0);
        float mip_level = mix(5.0, 2.0, f_shininess / 128.0);
        sky_contribution = textureLod(skybox_sampler, sky_sample_vector, mip_level).xyz * sky_percentage;
    }

    vec4 rim_lighting = rim_lighting(tangent_view_direction, tangent_space_normal);

    //Sun + skybox contribution
    vec3 environment_lighting = sun_color * ((sun_specular + sun_diffuse) * shadow_factor + sky_contribution + ambient_strength);

    //Sample the albedo map for the fragment's base color
    vec3 base_color = albedo_sample.xyz;
    float alpha = albedo_sample.a;
    vec3 final_color = (environment_lighting + point_lights_contribution) * base_color;
    frag_color = vec4(final_color, alpha) + rim_lighting;
}

void shade_toon() {
    vec4 albedo_sample = texture(albedo_sampler, f_uvs);
    if (visualize_albedo) {
        frag_color = albedo_sample;
        return;
    }

    vec3 tangent_space_normal = tangent_space_normal();
    vec3 tangent_view_direction = normalize(tangent_view_position - f_tan_pos);
    
    //Early exit if we're visualizing normals
    if (visualize_normals) {
        frag_color = vec4(tangent_space_normal * 0.5 + 0.5, 1.0);
        return;
    }

    //Diffuse contribution from the sun
    float sun_diffuse = toon_diffuse(tangent_sun_direction, tangent_space_normal);

    //Roughness is a [0, 1] value that gets mapped to [shininess_upper_bound, shininess_lower_bound]
    float roughness = texture(roughness_sampler, f_uvs).x;
    float f_shininess = (1.0 - roughness) * (shininess_upper_bound - shininess_lower_bound) + shininess_lower_bound;
    float specular_switch = roughness > 0.95 ? 0.0 : 1.0;
    
    //Specular contribution from the sun
    float sun_specular = specular_switch * toon_specular(tangent_view_direction, tangent_sun_direction, tangent_space_normal, f_shininess);

    //Compute sun shadow factor
    vec2 sun_shadowing = compute_sun_shadowing(tangent_space_normal);
    float shadow_factor = sun_shadowing.x;
    int shadow_cascade = int(sun_shadowing.y);

    if (visualize_cascade_zone) {
        visualize_shadow_cascade(sun_diffuse, ambient_strength, shadow_factor, shadow_cascade);
        return;
    }

    //Early exit for shadow visualization
    if (visualize_shadowed) {
        frag_color = vec4(vec3(shadow_factor), 1.0);
        return;
    }

    //Compute lighting from point lights
    vec3 point_lights_contribution = vec3(0.0);
    for (int i = 0; i < point_lights_count; i++) {
        vec4 direction_and_falloff = point_light_info(i);
        vec3 tangent_light_direction = direction_and_falloff.xyz;
        float falloff = direction_and_falloff.a;

        float diffuse = toon_diffuse(tangent_light_direction, tangent_space_normal);
        float specular = specular_switch * toon_specular(tangent_view_direction, tangent_light_direction, tangent_space_normal, f_shininess);
        falloff = smoothstep(0.2, 0.3, falloff);

        vec3 light_color = point_lights.colors[i];  //Light color
        point_lights_contribution += light_color * (diffuse + specular) * falloff;
    }

    vec4 rim_lighting = rim_lighting(tangent_view_direction, tangent_space_normal);

    vec3 sun_total = sun_color * ((sun_diffuse + sun_specular) * shadow_factor + ambient_strength);
    vec3 color = albedo_sample.rgb * (sun_total + point_lights_contribution);
    frag_color = vec4(color, albedo_sample.a) + rim_lighting;
}

void main() {
    if (toon_shading) {
        shade_toon();
    } else {
        shade_blinn_phong();
    }
}