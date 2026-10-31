// Playwright reporter that streams results as GitHub Actions annotations.
//
// A finished-job log is not reachable from the environment this project is
// developed in, but annotations are, and this reporter emits them while the
// tests run — so a failure is readable within seconds of happening instead of
// after the job finishes.
//
// Usage: npx playwright test --reporter=./scripts/e2e-live-reporter.mjs

const esc = (value) =>
  String(value)
    .replace(/%/g, "%25")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");

const firstMessageLine = (result) => {
  const text = [
    ...(result.errors ?? []).map((e) => e.message ?? ""),
    ...(result.error ? [result.error.message ?? String(result.error)] : []),
    result.status === "timedOut" ? "timed out" : "",
  ]
    .join("\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .find((line) => !/^at /.test(line));
  return (text ?? result.status ?? "failed").slice(0, 400);
};

export default class LiveReporter {
  onBegin(config, suite) {
    const total = suite.allTests().length;
    console.log(`::notice title=Playwright::running ${total} test(s)`);
  }

  onTestEnd(test, result) {
    const project = test.parent?.project?.()?.name ?? "";
    const file = test.location?.file ?? "";
    const title = `${project ? `[${project}] ` : ""}${test.title}`;

    if (result.status === "passed") {
      console.log(`ok   ${title}`);
      return;
    }
    if (result.status === "skipped") {
      console.log(`skip ${title}`);
      return;
    }
    console.log(`::error file=${esc(file)},title=Playwright ${esc(project)}::${esc(title)} — ${esc(firstMessageLine(result))}`);
    for (const [i, line] of String(firstMessageLine(result)).split("\n").slice(0, 6).entries()) {
      console.log(`::warning title=Playwright ${esc(project)} detail ${i + 1}::${esc(line)}`);
    }
  }

  onEnd(result) {
    console.log(`::notice title=Playwright::${esc(result.status)} in ${Math.round((result.duration ?? 0) / 1000)}s`);
  }

  onError(error) {
    console.log(`::error title=Playwright runner::${esc(firstMessageLine({ error, status: "failed" }))}`);
  }
}
