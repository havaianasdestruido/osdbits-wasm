import { expect, test } from "@playwright/test";

test("Phase 1 smoke: WASM handshake, IndexedDB, AudioContext, and WebGPU adapter render/readback", async ({
  page,
}) => {
  await page.goto("/");
  const app = page.locator("#app");
  await expect(app).toHaveAttribute("data-app-state", "ready");
  await expect(app).toHaveAttribute("data-abi-magic", "0x4f534442");
  await expect(app).toHaveAttribute("data-abi-version", "1");

  const capabilities = await page.evaluate(async () => {
    // 1. Verify IndexedDB write + readback
    const dbValue = await new Promise<string>((resolve, reject) => {
      const req = indexedDB.open("osdbits-smoke-db", 1);
      req.onupgradeneeded = () => {
        req.result.createObjectStore("kv");
      };
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const db = req.result;
        const tx = db.transaction("kv", "readwrite");
        tx.objectStore("kv").put("ps2-ok", "probe");
        tx.oncomplete = () => {
          const rtx = db.transaction("kv", "readonly");
          const getReq = rtx.objectStore("kv").get("probe");
          getReq.onsuccess = () => {
            const val = String(getReq.result);
            db.close();
            indexedDB.deleteDatabase("osdbits-smoke-db");
            resolve(val);
          };
          getReq.onerror = () => reject(getReq.error);
        };
        tx.onerror = () => reject(tx.error);
      };
    });

    // 2. Verify AudioContext availability
    const AudioCtx = window.AudioContext;
    const hasAudioContext = typeof AudioCtx === "function";
    let initialAudioState = "unavailable";
    if (hasAudioContext) {
      const ctx = new AudioCtx();
      initialAudioState = ctx.state;
      await ctx.close();
    }

    // 3. Verify WebGPU adapter + offscreen render + pixel readback
    const hasGpu = "gpu" in navigator;
    if (!hasGpu) {
      return { dbValue, hasAudioContext, initialAudioState, hasGpu, gpuPixel: null, isFallbackAdapter: null };
    }
    const adapter =
      (await navigator.gpu.requestAdapter({ powerPreference: "high-performance" })) ??
      (await navigator.gpu.requestAdapter({ forceFallbackAdapter: true }));
    if (!adapter) {
      return { dbValue, hasAudioContext, initialAudioState, hasGpu, gpuPixel: null, isFallbackAdapter: null };
    }
    const device = await adapter.requestDevice();
    const tex = device.createTexture({
      size: [4, 4],
      format: "rgba8unorm",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    const readBuf = device.createBuffer({
      size: 256 * 4,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const module = device.createShaderModule({
      code: `
        @vertex fn vs(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
          var p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
          return vec4f(p[idx], 0.5, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4f {
          return vec4f(16.0 / 255.0, 96.0 / 255.0, 212.0 / 255.0, 1.0);
        }
      `,
    });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs" },
      fragment: { module, entryPoint: "fs", targets: [{ format: "rgba8unorm" }] },
    });
    const enc = device.createCommandEncoder();
    const pass = enc.beginRenderPass({
      colorAttachments: [
        {
          view: tex.createView(),
          loadOp: "clear",
          storeOp: "store",
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.draw(3);
    pass.end();
    enc.copyTextureToBuffer({ texture: tex }, { buffer: readBuf, bytesPerRow: 256 }, [4, 4]);
    device.queue.submit([enc.finish()]);
    await readBuf.mapAsync(GPUMapMode.READ);
    const gpuPixel = Array.from(new Uint8Array(readBuf.getMappedRange(), 0, 4));
    readBuf.unmap();
    tex.destroy();
    readBuf.destroy();

    return {
      dbValue,
      hasAudioContext,
      initialAudioState,
      hasGpu,
      gpuPixel,
      isFallbackAdapter: adapter.info?.isFallbackAdapter ?? null,
    };
  });

  expect(capabilities.dbValue).toBe("ps2-ok");
  expect(capabilities.hasAudioContext).toBe(true);
  expect(capabilities.hasGpu).toBe(true);
  expect(capabilities.gpuPixel).toEqual([16, 96, 212, 255]);
});
