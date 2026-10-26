// Run the pinned Nexus account/catalogue modules with Node 24, without Electron.
import { registerHooks, stripTypeScriptTypes } from 'node:module';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const nexusRoot = new URL('../../../xodus-nexus/', import.meta.url);
registerHooks({
  resolve(specifier, context, next) {
    if (context.parentURL?.startsWith(nexusRoot.href) && specifier.endsWith('.js') && specifier.startsWith('.')) {
      const candidate = new URL(specifier.slice(0, -3) + '.ts', context.parentURL);
      if (existsSync(candidate)) return { url: candidate.href, shortCircuit: true };
    }
    return next(specifier, context);
  },
  load(url, context, next) {
    if (url.startsWith(nexusRoot.href) && url.endsWith('.ts')) {
      return { format: 'module', shortCircuit: true,
        source: stripTypeScriptTypes(readFileSync(fileURLToPath(url), 'utf8'), { mode: 'transform', sourceUrl: url }) };
    }
    return next(url, context);
  },
});

export const { CatalogService } = await import(new URL('src/main/catalog.ts', nexusRoot));
export const { AccountLibrary, parseEntitlements, entitlementKind } = await import(new URL('src/main/account-library.ts', nexusRoot));
export const { readInstallPlan, installSelection } = await import(new URL('src/main/install-plan.ts', nexusRoot));
export const { parseSubscription } = await import(new URL('src/shared/subscription.ts', nexusRoot));
