import blitPostWgsl from "./blit_post.wgsl?raw";
import fogSpriteWgsl from "./fog_sprite.wgsl?raw";
import glassCubeWgsl from "./glass_cube.wgsl?raw";
import lineTrailWgsl from "./line_trail.wgsl?raw";
import menuRodCubeWgsl from "./menu_rod_cube.wgsl?raw";
import towerWgsl from "./tower.wgsl?raw";
import uiOverlayWgsl from "./ui_overlay.wgsl?raw";

export const SHADER_SOURCES = {
  tower: towerWgsl,
  fogSprite: fogSpriteWgsl,
  lineTrail: lineTrailWgsl,
  glassCube: glassCubeWgsl,
  menuRodCube: menuRodCubeWgsl,
  blitPost: blitPostWgsl,
  uiOverlay: uiOverlayWgsl,
} as const;

export type ShaderModuleName = keyof typeof SHADER_SOURCES;
