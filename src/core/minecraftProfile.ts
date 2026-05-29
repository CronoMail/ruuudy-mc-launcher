export type LauncherProfile = {
  name: string;
  type: "custom" | string;
  created?: string;
  lastUsed?: string;
  lastVersionId: string;
  gameDir?: string;
  icon?: string;
};

export type LauncherProfilesFile = {
  profiles: Record<string, LauncherProfile>;
  settings?: Record<string, unknown>;
  version?: number;
};

export type LauncherProfileInput = {
  profileId: string;
  profileName: string;
  gameDir: string;
  minecraftVersion: string;
  loaderType?: "vanilla" | "fabric" | "forge" | "neoforge";
  loaderVersion: string;
};

export function createOrUpdateLauncherProfile(
  profilesFile: LauncherProfilesFile,
  input: LauncherProfileInput
): LauncherProfilesFile {
  const now = new Date().toISOString();
  const previous = profilesFile.profiles[input.profileId];

  return {
    ...profilesFile,
    profiles: {
      ...profilesFile.profiles,
      [input.profileId]: {
        name: input.profileName,
        type: "custom",
        created: previous?.created ?? now,
        lastUsed: previous?.lastUsed ?? now,
        icon: previous?.icon,
        gameDir: input.gameDir,
        lastVersionId: loaderVersionId(
          input.loaderType ?? "fabric",
          input.loaderVersion,
          input.minecraftVersion
        )
      }
    }
  };
}

export function fabricVersionId(loaderVersion: string, minecraftVersion: string): string {
  return loaderVersionId("fabric", loaderVersion, minecraftVersion);
}

export function loaderVersionId(
  loaderType: "vanilla" | "fabric" | "forge" | "neoforge",
  loaderVersion: string,
  minecraftVersion: string
): string {
  switch (loaderType) {
    case "vanilla":
      return minecraftVersion;
    case "fabric":
      return `fabric-loader-${loaderVersion}-${minecraftVersion}`;
    case "forge":
      return `${minecraftVersion}-forge-${loaderVersion}`;
    case "neoforge":
      return `neoforge-${loaderVersion}`;
  }
}
