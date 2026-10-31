import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export async function preparePlaywrightChromiumOptions() {
  const binDir = join(repoRoot, "node_modules", "@sparticuz", "chromium", "bin");
  if (process.platform !== "linux" || process.arch !== "x64" || !existsSync(binDir)) {
    return {
      launchOptions: {
        headless: true,
        args: [
          "--enable-unsafe-webgpu",
          "--ignore-gpu-blocklist",
        ],
      },
    };
  }

  const sparticuz = await import("@sparticuz/chromium");
  const chromiumPkg = sparticuz.default;
  await sparticuz.inflate(join(binDir, "al2023.tar.br"));
  await sparticuz.inflate(join(binDir, "swiftshader.tar.br"));
  await sparticuz.inflate(join(binDir, "fonts.tar.br"));
  const executablePath = await chromiumPkg.executablePath(binDir);
  const tmp = tmpdir();
  const al2023Lib = join(tmp, "al2023", "lib");
  sparticuz.setupLambdaEnvironment(al2023Lib);
  const vkIcd = join(tmp, "vk_swiftshader_icd.json");

  const filteredBaseArgs = chromiumPkg.args.filter(
    (arg) =>
      arg !== "--single-process" &&
      arg !== "--in-process-gpu" &&
      !arg.startsWith("--headless") &&
      !arg.startsWith("--enable-features=") &&
      !arg.startsWith("--use-gl=") &&
      !arg.startsWith("--use-angle="),
  );

  return {
    launchOptions: {
      executablePath,
      headless: true,
      env: {
        ...process.env,
        FONTCONFIG_PATH: join(tmp, "fonts"),
        VK_ICD_FILENAMES: vkIcd,
        VK_DRIVER_FILES: vkIcd,
        LD_LIBRARY_PATH: `${tmp}:${al2023Lib}:${process.env.LD_LIBRARY_PATH ?? ""}`,
      },
      args: [
        ...filteredBaseArgs,
        "--enable-features=SharedArrayBuffer,Vulkan,WebGPU",
        "--enable-unsafe-webgpu",
        "--enable-unsafe-swiftshader",
        "--use-vulkan=swiftshader",
        "--use-webgpu-adapter=swiftshader",
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--autoplay-policy=user-gesture-required",
      ],
    },
  };
}
