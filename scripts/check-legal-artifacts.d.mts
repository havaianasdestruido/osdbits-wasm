export function scanDirectoryForProprietaryArtifacts(rootDir: string): {
  violations: Array<{ path: string; reason: string }>;
};

export function verifyHistoricalProvenance(repoRoot: string): {
  ok: boolean;
  details: string;
};
