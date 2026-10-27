export function externalLauncherEnvironment(source: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const env = { ...source }; delete env.NODE_OPTIONS; delete env.ELECTRON_RUN_AS_NODE; return env;
}
