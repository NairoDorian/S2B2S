import fs from 'fs';
import path from 'path';

const packageJsonPath = path.resolve(__dirname, '../package.json');
const cargoTomlPath = path.resolve(__dirname, '../src-tauri/Cargo.toml');

async function getLatestNpmVersion(pkg: string): Promise<string | null> {
  try {
    const res = await fetch(`https://registry.npmjs.org/${pkg}/latest`);
    if (!res.ok) return null;
    const json = (await res.json()) as any;
    return json.version || null;
  } catch (e) {
    return null;
  }
}

async function getLatestCratesIoVersion(crate: string): Promise<string | null> {
  try {
    const res = await fetch(`https://crates.io/api/v1/crates/${crate}`, {
      headers: {
        'User-Agent': 's2b2s-dependency-updater (contact@s2b2s.local)'
      }
    });
    if (!res.ok) return null;
    const json = (await res.json()) as any;
    return json.crate?.max_stable_version || json.crate?.max_version || null;
  } catch (e) {
    return null;
  }
}

function parseCargoTomlDeps(content: string): string[] {
  const crates = new Set<string>();
  const lines = content.split('\n');
  let inDepsSection = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
      const section = trimmed.slice(1, -1).toLowerCase();
      if (
        section.includes('dependencies') ||
        section.includes('build-dependencies') ||
        section.includes('dev-dependencies')
      ) {
        inDepsSection = true;
      } else {
        inDepsSection = false;
      }
      continue;
    }

    if (inDepsSection && trimmed && !trimmed.startsWith('#')) {
      const parts = trimmed.split('=');
      const name = parts[0].trim();
      if (name) {
        crates.add(name);
      }
    }
  }

  // Filter out any target-specific sections or special cases that were parsed incorrectly
  return Array.from(crates).filter(c => !c.includes('[') && !c.includes(']') && c !== 'git' && c !== 'version' && c !== 'branch');
}

async function main() {
  console.log('--- NPM Dependencies ---');
  if (fs.existsSync(packageJsonPath)) {
    const pkgJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    const allDeps = { ...(pkgJson.dependencies || {}), ...(pkgJson.devDependencies || {}) };
    for (const [name, version] of Object.entries(allDeps)) {
      const latest = await getLatestNpmVersion(name);
      console.log(`${name}: Current = ${version}, Latest = ${latest}`);
    }
  } else {
    console.log('package.json not found!');
  }

  console.log('\n--- Rust Crates ---');
  if (fs.existsSync(cargoTomlPath)) {
    const cargoContent = fs.readFileSync(cargoTomlPath, 'utf8');
    const crates = parseCargoTomlDeps(cargoContent);
    for (const crate of crates) {
      const latest = await getLatestCratesIoVersion(crate);
      console.log(`${crate}: Latest = ${latest}`);
    }
  } else {
    console.log('Cargo.toml not found!');
  }
}

main();
