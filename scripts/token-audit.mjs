import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.cwd();
const sourceRoot = join(root, "src");
const tokensPath = join(sourceRoot, "styles", "tokens.css");

function walk(directory) {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

const cssFiles = walk(sourceRoot).filter((path) => path.endsWith(".css"));
const tokensSource = readFileSync(tokensPath, "utf8");
const tokenNames = Array.from(tokensSource.matchAll(/--[a-z0-9-]+(?=\s*:)/g), (match) => match[0]);
const used = new Set();
const findings = [];

const colorPattern = /#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch)\(/;
const gradientPattern = /\b(?:linear|radial|conic)-gradient\(/;
const transitionAllPattern = /transition\s*:\s*all\b/;
const uppercasePattern = /text-transform\s*:\s*uppercase\b/;
const trackedPattern = /letter-spacing\s*:\s*([^;]+);?/;
const rootPattern = /:root\b/;
const layoutProperties = new Set(["width", "height", "top", "left"]);

function checkTransitionList(value, file, lineNumber, kind) {
  const properties = value
    .split(",")
    .map((part) => part.trim().split(/\s+/)[0])
    .filter(Boolean);
  for (const property of properties) {
    if (layoutProperties.has(property)) {
      findings.push({
        file,
        line: lineNumber,
        message: `${kind} animates ${property}; use transform or opacity`,
      });
    }
  }
}

for (const path of cssFiles) {
  const file = relative(root, path).replaceAll("\\", "/");
  const isTokens = path === tokensPath;
  const source = readFileSync(path, "utf8");

  if (!isTokens && rootPattern.test(source)) {
    findings.push({ file, line: 1, message: "declares :root outside tokens.css" });
  }

  for (const match of source.matchAll(/var\((--[a-z0-9-]+)/g)) {
    used.add(match[1]);
  }

  const lines = source.split(/\r?\n/);
  lines.forEach((raw, index) => {
    const line = raw.trim();
    const lineNumber = index + 1;
    if (!line || line.startsWith("/*") || line.startsWith("*")) return;

    if (!isTokens && colorPattern.test(line)) {
      findings.push({ file, line: lineNumber, message: "literal color outside tokens.css" });
    }
    if (!isTokens && gradientPattern.test(line)) {
      findings.push({ file, line: lineNumber, message: "gradient outside tokens.css" });
    }
    if (!isTokens) {
      const font = line.match(/font-family\s*:\s*([^;]+);?/);
      if (font && !font[1].trim().startsWith("var(")) {
        findings.push({
          file,
          line: lineNumber,
          message: "literal font-family outside tokens.css",
        });
      }
    }
    if (transitionAllPattern.test(line)) {
      findings.push({ file, line: lineNumber, message: "transition: all" });
    }
    if (uppercasePattern.test(line)) {
      findings.push({ file, line: lineNumber, message: "tracked capitals via text-transform" });
    }
    const tracking = line.match(trackedPattern);
    if (!isTokens && tracking) {
      const value = tracking[1].trim();
      if (value !== "0" && value !== "normal" && !value.startsWith("-")) {
        findings.push({
          file,
          line: lineNumber,
          message: "positive letter-spacing on running text",
        });
      }
    }

    const transition = line.match(/transition(?:-property)?\s*:\s*([^;]+);?/);
    if (transition) checkTransitionList(transition[1], file, lineNumber, "transition");

    const animation = line.match(/animation(?:-name)?\s*:\s*([^;]+);?/);
    if (animation) checkTransitionList(animation[1], file, lineNumber, "animation");
  });

  if (/@keyframes/.test(source)) {
    for (const block of source.matchAll(/@keyframes[^{]+\{([\s\S]*?)\n\}/g)) {
      const body = block[1];
      for (const property of layoutProperties) {
        if (new RegExp(`\\b${property}\\s*:`).test(body)) {
          const offset = source.slice(0, block.index ?? 0).split(/\r?\n/).length;
          findings.push({
            file,
            line: offset,
            message: `keyframes animate ${property}; use transform or opacity`,
          });
        }
      }
    }
  }
}

const unused = tokenNames.filter((name) => !used.has(name));

if (findings.length > 0) {
  console.error(`Token audit: ${findings.length} finding(s)`);
  for (const finding of findings) {
    console.error(`  ${finding.file}:${finding.line} ${finding.message}`);
  }
}

if (unused.length > 0) {
  console.error(`Token audit: ${unused.length} unused token(s)`);
  for (const name of unused) console.error(`  ${name}`);
}

if (findings.length === 0 && unused.length === 0) {
  console.log(`Token audit: clean (${cssFiles.length} stylesheets, ${tokenNames.length} tokens)`);
}

process.exit(findings.length === 0 && unused.length === 0 ? 0 : 1);
