import path from "node:path";
import { createRequire } from "node:module";

const VIRTUAL_ROOT = "/__monochange_typescript";

const input = await readStandardInput();
const analysisRequest = JSON.parse(input);

try {
	const ts = loadTypeScript(analysisRequest);
	const before = buildEndpoint(ts, analysisRequest, analysisRequest.before, "before");
	const after = buildEndpoint(ts, analysisRequest, analysisRequest.after, "after");
	const response = compareEndpoints(ts, analysisRequest, before, after);

	process.stdout.write(JSON.stringify(response));
} catch (error) {
	process.stdout.write(
		JSON.stringify({
			version: null,
			fallback: true,
			coverage: "TypeScript declaration compatibility was unavailable",
			fallbackReason: errorMessage(error),
			warnings: [],
			changes: [],
		}),
	);
}

async function readStandardInput() {
	let value = "";

	for await (const chunk of process.stdin) {
		value += chunk;
	}

	return value;
}

function loadTypeScript(request) {
	const localRequire = createRequire(import.meta.url);
	const searchPaths = [
		path.join(request.repoRoot, request.packageRoot),
		request.repoRoot,
		process.cwd(),
	];
	const modulePath = localRequire.resolve("typescript", { paths: searchPaths });

	return localRequire(modulePath);
}

function buildEndpoint(ts, request, snapshot, side) {
	const workspaceRoot = path.posix.join(VIRTUAL_ROOT, side);
	const packageRoot = path.posix.join(workspaceRoot, request.packageRoot);
	const files = new Map(
		snapshot.files.map((file) => [
			path.posix.join(packageRoot, normalizeRelativePath(file.path)),
			file.contents,
		]),
	);
	const externalInputs = new Set();
	const externalFiles = new Map();
	const hostAccess = createHostAccess(
		ts,
		request.repoRoot,
		workspaceRoot,
		packageRoot,
		files,
		externalInputs,
		externalFiles,
	);
	const emitted = emitDeclarations(ts, packageRoot, files, hostAccess);
	const manifestPath = path.posix.join(packageRoot, "package.json");
	const manifestText = files.get(manifestPath);

	if (manifestText === undefined) {
		return failedEndpoint(
			ts,
			files,
			packageRoot,
			"package.json was not present in the package snapshot",
		);
	}

	let manifest;

	try {
		manifest = JSON.parse(manifestText);
	} catch (error) {
		return failedEndpoint(
			ts,
			files,
			packageRoot,
			`package.json could not be parsed: ${errorMessage(error)}`,
		);
	}

	const emittedErrors = emitted.errors.filter(
		(error) =>
			error !== "no TypeScript sources or declaration files were found" ||
			manifestDeclaresTypedSurface(manifest),
	);

	if (emittedErrors.length > 0) {
		return {
			version: ts.version,
			files: new Map([...emitted.files, [manifestPath, manifestText], ...externalFiles]),
			packageRoot,
			entrypoints: new Map(),
			externalInputs,
			errors: emittedErrors,
			failed: true,
		};
	}

	const entrypoints = collectEntrypoints(
		manifest,
		emitted.files,
		emitted.sourceDeclarations,
		packageRoot,
	);

	return {
		version: ts.version,
		files: new Map([...emitted.files, [manifestPath, manifestText], ...externalFiles]),
		packageRoot,
		entrypoints: entrypoints.targets,
		externalInputs,
		errors: entrypoints.errors,
		failed: entrypoints.fatal,
	};
}

function failedEndpoint(ts, files, packageRoot, message) {
	return {
		version: ts.version,
		files,
		packageRoot,
		entrypoints: new Map(),
		externalInputs: new Set(),
		errors: [message],
		failed: true,
	};
}

function createHostAccess(
	ts,
	repoRoot,
	workspaceRoot,
	packageRoot,
	files,
	externalInputs,
	externalFiles,
) {
	function isSnapshotOwned(fileName) {
		const normalized = normalizeAbsolutePath(fileName);

		return (
			isPathInside(normalized, packageRoot) &&
			!normalized.slice(packageRoot.length).split("/").includes("node_modules")
		);
	}

	function translatedPath(fileName) {
		const normalized = normalizeAbsolutePath(fileName);

		if (!isPathInside(normalized, workspaceRoot)) {
			return null;
		}

		const relative = path.posix.relative(workspaceRoot, normalized);

		return path.join(repoRoot, ...relative.split("/"));
	}

	function fileExists(fileName) {
		const normalized = normalizeAbsolutePath(fileName);

		if (files.has(normalized)) {
			return true;
		}
		if (isSnapshotOwned(normalized)) {
			return false;
		}

		const translated = translatedPath(normalized);

		if (translated !== null && ts.sys.fileExists(translated)) {
			externalInputs.add(normalized);

			return true;
		}

		return ts.sys.fileExists(fileName);
	}

	function readFile(fileName) {
		const normalized = normalizeAbsolutePath(fileName);
		const inMemory = files.get(normalized);

		if (inMemory !== undefined) {
			return inMemory;
		}
		if (isSnapshotOwned(normalized)) {
			return undefined;
		}

		const translated = translatedPath(normalized);

		if (translated !== null && ts.sys.fileExists(translated)) {
			externalInputs.add(normalized);
			const contents = ts.sys.readFile(translated);

			if (contents !== undefined) {
				externalFiles.set(normalized, contents);
			}

			return contents;
		}

		return ts.sys.readFile(fileName);
	}

	function directoryExists(directoryName) {
		const normalized = normalizeAbsolutePath(directoryName);
		const prefix = normalized.endsWith("/") ? normalized : `${normalized}/`;

		if ([...files.keys()].some((fileName) => fileName.startsWith(prefix))) {
			return true;
		}
		if (isSnapshotOwned(normalized)) {
			return false;
		}

		const translated = translatedPath(normalized);

		return translated === null
			? (ts.sys.directoryExists?.(directoryName) ?? false)
			: (ts.sys.directoryExists?.(translated) ?? false);
	}

	function readDirectory(rootDir, extensions, excludes, includes, depth) {
		const normalizedRoot = normalizeAbsolutePath(rootDir);
		const prefix = normalizedRoot.endsWith("/") ? normalizedRoot : `${normalizedRoot}/`;
		const inMemory = [...files.keys()].filter((fileName) => {
			if (!fileName.startsWith(prefix)) {
				return false;
			}

			return (
				extensions === undefined || extensions.some((extension) => fileName.endsWith(extension))
			);
		});
		const translated = translatedPath(normalizedRoot);

		if (isSnapshotOwned(normalizedRoot)) {
			return inMemory;
		}
		if (translated === null || !ts.sys.directoryExists?.(translated)) {
			return inMemory;
		}
		externalInputs.add(normalizedRoot);

		const fromDisk = ts.sys
			.readDirectory(translated, extensions, excludes, includes, depth)
			.map((fileName) => {
				const relative = normalizeRelativePath(path.relative(repoRoot, fileName));

				return path.posix.join(workspaceRoot, relative);
			});

		return [...new Set([...inMemory, ...fromDisk])];
	}

	return { fileExists, readFile, directoryExists, readDirectory };
}

function emitDeclarations(ts, packageRoot, sourceFiles, access) {
	const configPath = path.posix.join(packageRoot, "tsconfig.json");
	const configText = sourceFiles.get(configPath);
	let rootNames;
	let options;
	let configErrors = [];

	if (configText === undefined) {
		rootNames = [...sourceFiles.keys()].filter(isTypeScriptSource);
		options = {
			strict: true,
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.NodeNext,
			moduleResolution: ts.ModuleResolutionKind.NodeNext,
			rootDir: path.posix.join(packageRoot, "src"),
			outDir: path.posix.join(packageRoot, "dist"),
		};
	} else {
		const parsedText = ts.parseConfigFileTextToJson(configPath, configText);

		if (parsedText.error !== undefined) {
			return {
				files: new Map(sourceFiles),
				errors: [formatDiagnostic(ts, parsedText.error)],
			};
		}

		const parseHost = {
			useCaseSensitiveFileNames: true,
			fileExists: access.fileExists,
			readFile: access.readFile,
			readDirectory: access.readDirectory,
		};
		const parsed = ts.parseJsonConfigFileContent(
			parsedText.config,
			parseHost,
			packageRoot,
			{},
			configPath,
		);
		rootNames = parsed.fileNames;
		options = parsed.options;
		configErrors = parsed.errors;
	}

	if (rootNames.length === 0) {
		const declarations = new Map(
			[...sourceFiles].filter(([fileName]) => isDeclarationFile(fileName)),
		);

		return {
			files: declarations,
			sourceDeclarations: new Map(),
			errors:
				declarations.size === 0 ? ["no TypeScript sources or declaration files were found"] : [],
		};
	}

	options = {
		...options,
		declaration: true,
		declarationMap: false,
		emitDeclarationOnly: true,
		incremental: false,
		composite: false,
		noEmit: false,
		noEmitOnError: true,
		sourceMap: false,
		tsBuildInfoFile: undefined,
	};
	const baseHost = ts.createCompilerHost(options, true);
	const host = {
		...baseHost,
		fileExists: access.fileExists,
		readFile: access.readFile,
		directoryExists: access.directoryExists,
		readDirectory: access.readDirectory,
		getSourceFile(fileName, languageVersion, onError) {
			const text = access.readFile(fileName);

			if (text === undefined) {
				onError?.(`could not read ${fileName}`);

				return undefined;
			}

			return ts.createSourceFile(
				fileName,
				text,
				languageVersion,
				true,
				ts.getScriptKindFromFileName(fileName),
			);
		},
	};
	const program = ts.createProgram({
		rootNames,
		options,
		host,
		configFileParsingDiagnostics: configErrors,
	});
	const diagnostics = ts
		.getPreEmitDiagnostics(program)
		.filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error);
	const outputFiles = new Map([...sourceFiles].filter(([fileName]) => isDeclarationFile(fileName)));
	const sourceDeclarations = new Map();
	const emitResult = program.emit(
		undefined,
		(fileName, contents, _writeByteOrderMark, _onError, emittedSources) => {
			const outputPath = normalizeAbsolutePath(fileName);
			outputFiles.set(outputPath, contents);
			for (const source of emittedSources ?? []) {
				sourceDeclarations.set(normalizeAbsolutePath(source.fileName), outputPath);
			}
		},
		undefined,
		true,
	);
	const emitErrors = emitResult.diagnostics.filter(
		(diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
	);
	const errors = [...diagnostics, ...emitErrors].map((diagnostic) =>
		formatDiagnostic(ts, diagnostic),
	);

	return {
		files: outputFiles,
		sourceDeclarations,
		errors: [...new Set(errors)],
	};
}

function collectEntrypoints(manifest, files, sourceDeclarations, packageRoot) {
	const targets = new Map();
	const errors = [];
	const exportsField = manifest.exports;

	function addTarget(publicPath, conditions, rawTarget) {
		if (typeof rawTarget !== "string") {
			return false;
		}

		const resolved = resolveDeclarationTarget(rawTarget, files, sourceDeclarations, packageRoot);

		if (resolved === null) {
			return false;
		}

		const suffix = conditions.length === 0 ? "" : `[${conditions.join(".")}]`;
		const key = `${publicPath}${suffix}`;
		targets.set(key, resolved);

		return true;
	}

	function visitExport(publicPath, value, conditions) {
		if (typeof value === "string") {
			return addTarget(publicPath, conditions, value);
		}

		if (Array.isArray(value)) {
			for (const candidate of value) {
				if (visitExport(publicPath, candidate, conditions)) {
					return true;
				}
			}

			return false;
		}

		if (value === null || typeof value !== "object") {
			return false;
		}

		if (Object.hasOwn(value, "types")) {
			return visitExport(publicPath, value.types, conditions);
		}

		let found = false;

		for (const [condition, nested] of Object.entries(value)) {
			if (condition.startsWith(".")) {
				continue;
			}

			found = visitExport(publicPath, nested, [...conditions, condition]) || found;
		}

		return found;
	}

	if (exportsField !== undefined) {
		if (
			exportsField !== null &&
			typeof exportsField === "object" &&
			!Array.isArray(exportsField) &&
			Object.keys(exportsField).some((key) => key.startsWith("."))
		) {
			for (const [publicPath, value] of Object.entries(exportsField)) {
				if (publicPath.includes("*")) {
					errors.push(`wildcard export ${publicPath} could not be enumerated conclusively`);
					continue;
				}

				if (!visitExport(publicPath, value, [])) {
					errors.push(`no declaration target resolved for export ${publicPath}`);
				}
			}
		} else if (!visitExport(".", exportsField, [])) {
			errors.push("no declaration target resolved for the root export");
		}
	}

	const declaresTypedSurface = manifestDeclaresTypedSurface(manifest);

	if (targets.size === 0) {
		const typesTarget = manifest.types ?? manifest.typings;

		if (typeof typesTarget === "string") {
			if (!addTarget(".", [], typesTarget)) {
				errors.push(`declaration target ${typesTarget} was not emitted or present`);
			}
		} else if (!addTarget(".", [], "./index.d.ts") && declaresTypedSurface) {
			errors.push("package metadata does not identify a resolvable declaration entrypoint");
		}
	}

	return {
		targets,
		errors,
		fatal: targets.size === 0 && declaresTypedSurface && errors.length > 0,
	};
}

function exportsContainTypes(value) {
	if (typeof value === "string") {
		return isDeclarationFile(value) || isTypeScriptSource(value);
	}
	if (Array.isArray(value)) {
		return value.some(exportsContainTypes);
	}
	if (value === null || typeof value !== "object") {
		return false;
	}

	return Object.hasOwn(value, "types") || Object.values(value).some(exportsContainTypes);
}

function manifestDeclaresTypedSurface(manifest) {
	return (
		typeof (manifest.types ?? manifest.typings) === "string" ||
		exportsContainTypes(manifest.exports)
	);
}

function resolveDeclarationTarget(rawTarget, files, sourceDeclarations, packageRoot) {
	const normalized = normalizeRelativePath(rawTarget);
	const candidates = [normalized];
	const extension = path.posix.extname(normalized);

	if ([".js", ".jsx", ".mjs", ".cjs"].includes(extension)) {
		const stem = normalized.slice(0, -extension.length);
		candidates.push(`${stem}.d.ts`);
		candidates.push(`${stem}.d.mts`);
		candidates.push(`${stem}.d.cts`);
	}

	for (const candidate of candidates) {
		const absolute = path.posix.join(packageRoot, candidate);
		const emittedDeclaration = sourceDeclarations.get(absolute);

		if (emittedDeclaration !== undefined) {
			return emittedDeclaration;
		}

		if (files.has(absolute) && (isDeclarationFile(absolute) || isTypeScriptSource(absolute))) {
			return absolute;
		}
	}

	return null;
}

function compareEndpoints(ts, request, before, after) {
	const endpointErrors = [
		...before.errors.map((error) => `before: ${error}`),
		...after.errors.map((error) => `after: ${error}`),
	];

	if (
		before.failed ||
		after.failed ||
		(before.entrypoints.size === 0 && after.entrypoints.size === 0)
	) {
		return {
			version: ts.version,
			fallback: true,
			coverage: "TypeScript could not resolve and emit both declaration surfaces",
			fallbackReason: endpointErrors.join("; ") || "no typed entrypoint was available",
			warnings: endpointErrors,
			changes: [],
		};
	}

	const allFiles = new Map([...before.files, ...after.files]);
	const options = {
		strict: true,
		skipLibCheck: false,
		noEmit: true,
		target: ts.ScriptTarget.ES2022,
		module: ts.ModuleKind.NodeNext,
		moduleResolution: ts.ModuleResolutionKind.NodeNext,
		types: [],
	};
	const baseHost = ts.createCompilerHost(options, true);
	function translatedComparisonPath(fileName) {
		const normalized = normalizeAbsolutePath(fileName);

		for (const [side, endpoint] of [
			["before", before],
			["after", after],
		]) {
			const workspaceRoot = path.posix.join(VIRTUAL_ROOT, side);

			if (isPathInside(normalized, workspaceRoot)) {
				const relative = path.posix.relative(workspaceRoot, normalized);

				return {
					endpoint,
					normalized,
					real: path.join(request.repoRoot, ...relative.split("/")),
					workspaceRoot,
				};
			}
		}

		return null;
	}
	function hasVirtualDirectory(directoryName) {
		const normalized = normalizeAbsolutePath(directoryName);
		const prefix = normalized.endsWith("/") ? normalized : `${normalized}/`;

		return [...allFiles.keys()].some((fileName) => fileName.startsWith(prefix));
	}
	const host = {
		...baseHost,
		fileExists(fileName) {
			const normalized = normalizeAbsolutePath(fileName);

			if (allFiles.has(normalized)) {
				return true;
			}
			const access = translatedComparisonPath(normalized);

			if (access !== null && baseHost.fileExists(access.real)) {
				recordComparisonInput(access);

				return true;
			}

			return baseHost.fileExists(fileName);
		},
		readFile(fileName) {
			const normalized = normalizeAbsolutePath(fileName);
			const inMemory = allFiles.get(normalized);

			if (inMemory !== undefined) {
				return inMemory;
			}
			const access = translatedComparisonPath(normalized);

			if (access !== null && baseHost.fileExists(access.real)) {
				recordComparisonInput(access);

				return baseHost.readFile(access.real);
			}

			return baseHost.readFile(fileName);
		},
		directoryExists(directoryName) {
			if (hasVirtualDirectory(directoryName)) {
				return true;
			}
			const access = translatedComparisonPath(directoryName);

			return access !== null
				? (baseHost.directoryExists?.(access.real) ?? false)
				: (baseHost.directoryExists?.(directoryName) ?? false);
		},
		getDirectories(directoryName) {
			const normalized = normalizeAbsolutePath(directoryName);
			const prefix = normalized.endsWith("/") ? normalized : `${normalized}/`;
			const virtual = [...allFiles.keys()]
				.filter((fileName) => fileName.startsWith(prefix))
				.map((fileName) => fileName.slice(prefix.length).split("/")[0])
				.filter((segment) => segment.length > 0);
			const access = translatedComparisonPath(normalized);
			const disk =
				access === null
					? (baseHost.getDirectories?.(directoryName) ?? [])
					: (baseHost.getDirectories?.(access.real) ?? []).map((directory) =>
							path.posix.join(
								access.workspaceRoot,
								normalizeRelativePath(path.relative(request.repoRoot, directory)),
							),
						);

			return [...new Set([...virtual, ...disk])];
		},
		realpath(fileName) {
			const normalized = normalizeAbsolutePath(fileName);
			const access = translatedComparisonPath(normalized);

			return allFiles.has(normalized) ||
				hasVirtualDirectory(normalized) ||
				(access !== null && baseHost.fileExists(access.real))
				? normalized
				: (baseHost.realpath?.(fileName) ?? fileName);
		},
		getSourceFile(fileName, languageVersion, onError, shouldCreateNewSourceFile) {
			const normalized = normalizeAbsolutePath(fileName);
			const text = allFiles.get(normalized);

			if (text === undefined) {
				return baseHost.getSourceFile(
					fileName,
					languageVersion,
					onError,
					shouldCreateNewSourceFile,
				);
			}

			return ts.createSourceFile(
				normalized,
				text,
				languageVersion,
				true,
				ts.getScriptKindFromFileName(normalized),
			);
		},
	};
	const rootNames = [...new Set([...before.entrypoints.values(), ...after.entrypoints.values()])];
	const program = ts.createProgram({ rootNames, options, host });
	const diagnostics = ts
		.getPreEmitDiagnostics(program)
		.filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error)
		.map((diagnostic) => formatDiagnostic(ts, diagnostic));

	if (diagnostics.length > 0) {
		return {
			version: ts.version,
			fallback: true,
			coverage: "TypeScript could not type-check the emitted declaration surfaces",
			fallbackReason: diagnostics.join("; "),
			warnings: diagnostics,
			changes: [],
		};
	}

	const checker = program.getTypeChecker();
	const changes = [];
	const entrypointNames = new Set([...before.entrypoints.keys(), ...after.entrypoints.keys()]);

	for (const entrypoint of [...entrypointNames].toSorted()) {
		const beforePath = before.entrypoints.get(entrypoint);
		const afterPath = after.entrypoints.get(entrypoint);

		if (beforePath === undefined) {
			changes.push(
				changeForEntrypoint(
					"additive",
					"added",
					entrypoint,
					null,
					afterPath,
					"added typed entrypoint",
				),
			);
			continue;
		}

		if (afterPath === undefined) {
			changes.push(
				changeForEntrypoint(
					"breaking",
					"removed",
					entrypoint,
					beforePath,
					null,
					"removed typed entrypoint",
				),
			);
			continue;
		}

		changes.push(
			...compareEntrypoint(ts, checker, program, request, entrypoint, beforePath, afterPath),
		);
	}

	const partialReasons = [...endpointErrors];

	if (changes.length === 0) {
		changes.push({
			outcome: "compatible",
			suggestedBump: "none",
			kind: "modified",
			itemKind: "declaration_surface",
			itemPath: request.packageName,
			summary: "source changes preserve the emitted TypeScript declaration surface",
			filePath: "package.json",
			beforeSignature: null,
			afterSignature: null,
			confidence: "high",
			completeness: "complete",
			coverage: "all explicit typed entrypoints emitted and compared",
			fallbackReason: null,
		});
	}

	const externalSides = [
		before.externalInputs.size > 0 ? "before" : null,
		after.externalInputs.size > 0 ? "after" : null,
	].filter((side) => side !== null);
	if (externalSides.length > 0) {
		partialReasons.push(
			`${externalSides.join(" and ")} used config or dependency declarations from the current workspace`,
		);
	}

	if (partialReasons.length > 0) {
		changes.push(
			inconclusiveChange(
				request.packageName,
				partialReasons.join("; "),
				"some typed entrypoints or inputs were not fully checked against isolated snapshots",
			),
		);
	}

	return {
		version: ts.version,
		fallback: false,
		coverage:
			partialReasons.length === 0
				? "all explicit typed entrypoints emitted and compared"
				: "typed entrypoints were compared with partial snapshot isolation",
		fallbackReason: null,
		warnings: partialReasons,
		changes,
	};
}

function compareEntrypoint(ts, checker, program, request, entrypoint, beforePath, afterPath) {
	const beforeSymbols = exportsForFile(checker, program, beforePath);
	const afterSymbols = exportsForFile(checker, program, afterPath);
	const names = new Set([...beforeSymbols.keys(), ...afterSymbols.keys()]);
	const changes = [];

	for (const name of [...names].toSorted()) {
		const beforeSymbol = beforeSymbols.get(name);
		const afterSymbol = afterSymbols.get(name);
		const itemPath = `${entrypoint}#${name}`;

		if (beforeSymbol === undefined) {
			changes.push(
				changeForSymbol(
					ts,
					checker,
					request,
					"additive",
					"added",
					itemPath,
					null,
					afterSymbol,
					afterPath,
					"added public TypeScript export",
				),
			);
			continue;
		}

		if (afterSymbol === undefined) {
			changes.push(
				changeForSymbol(
					ts,
					checker,
					request,
					"breaking",
					"removed",
					itemPath,
					beforeSymbol,
					null,
					beforePath,
					"removed public TypeScript export",
				),
			);
			continue;
		}

		const beforeDescription = describeSymbol(ts, checker, beforeSymbol);
		const afterDescription = describeSymbol(ts, checker, afterSymbol);

		const sameSignature = beforeDescription.signature === afterDescription.signature;
		if (
			sameSignature &&
			(beforeDescription.genericType ||
				beforeDescription.nominalType ||
				afterDescription.genericType ||
				afterDescription.nominalType)
		) {
			continue;
		}

		const result = compareSymbolTypes(ts, checker, beforeDescription, afterDescription);
		if (sameSignature && result.outcome === "compatible") {
			continue;
		}
		changes.push({
			outcome: result.outcome,
			suggestedBump: result.suggestedBump,
			kind: "modified",
			itemKind: result.itemKind,
			itemPath,
			summary: `${result.summary} \`${itemPath}\``,
			filePath: relativeDeclarationPath(request, afterPath),
			beforeSignature: beforeDescription.signature,
			afterSignature: afterDescription.signature,
			confidence: result.confidence,
			completeness: result.completeness,
			coverage: result.coverage,
			fallbackReason: result.fallbackReason,
		});
	}

	return changes;
}

function exportsForFile(checker, program, fileName) {
	const sourceFile = program.getSourceFile(fileName);

	if (sourceFile === undefined) {
		return new Map();
	}

	const moduleSymbol = checker.getSymbolAtLocation(sourceFile);

	if (moduleSymbol === undefined) {
		return new Map();
	}

	return new Map(checker.getExportsOfModule(moduleSymbol).map((symbol) => [symbol.name, symbol]));
}

function describeSymbol(ts, checker, exportedSymbol) {
	const symbol =
		exportedSymbol.flags & ts.SymbolFlags.Alias
			? checker.getAliasedSymbol(exportedSymbol)
			: exportedSymbol;
	const declarations = symbol.declarations ?? exportedSymbol.declarations ?? [];
	const printer = ts.createPrinter({ removeComments: true });
	const signature = declarations
		.map((declaration) =>
			printer.printNode(ts.EmitHint.Unspecified, declaration, declaration.getSourceFile()),
		)
		.join("\n")
		.trim();
	const flags = symbol.flags | exportedSymbol.flags;
	const hasType = (flags & ts.SymbolFlags.Type) !== 0;
	const hasValue = (flags & ts.SymbolFlags.Value) !== 0;
	const genericType =
		hasType && declarations.some((declaration) => (declaration.typeParameters?.length ?? 0) > 0);
	const nominalType =
		hasType && declarations.some((declaration) => declarationHasNominalMembers(ts, declaration));
	const typeMembers = new Set(
		declarations.flatMap((declaration) => declarationMemberNames(ts, declaration)),
	);
	const memberShapes = new Map(
		declarations.flatMap((declaration) => declarationMemberShapes(ts, declaration)),
	);

	return {
		symbol,
		declaration: declarations[0],
		signature,
		hasType,
		hasValue,
		genericType,
		nominalType,
		typeMembers,
		memberShapes,
	};
}

function declarationMemberNames(ts, declaration) {
	if (!Array.isArray(declaration.members)) {
		return [];
	}

	return declaration.members.flatMap((member) => {
		const name = declarationMemberName(ts, member);

		return name === null ? [] : [name];
	});
}

function declarationMemberName(ts, member) {
	const name = member.name;

	if (
		name !== undefined &&
		(ts.isIdentifier(name) ||
			ts.isPrivateIdentifier(name) ||
			ts.isStringLiteral(name) ||
			ts.isNumericLiteral(name))
	) {
		return name.text;
	}

	return null;
}

function declarationMemberShapes(ts, declaration) {
	if (!Array.isArray(declaration.members)) {
		return [];
	}

	return declaration.members.flatMap((member) => {
		const name = declarationMemberName(ts, member);

		if (name === null) {
			return [];
		}

		return [
			[
				name,
				{
					optional: member.questionToken !== undefined,
					readonly:
						member.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword) ??
						false,
				},
			],
		];
	});
}

function declarationHasNominalMembers(ts, declaration) {
	let nominal = false;

	function visit(node) {
		if (ts.isPrivateIdentifier(node)) {
			nominal = true;
			return;
		}

		if (
			node.modifiers?.some(
				(modifier) =>
					modifier.kind === ts.SyntaxKind.PrivateKeyword ||
					modifier.kind === ts.SyntaxKind.ProtectedKeyword,
			)
		) {
			nominal = true;
			return;
		}

		ts.forEachChild(node, visit);
	}

	visit(declaration);

	return nominal;
}

function compareSymbolTypes(ts, checker, before, after) {
	const itemKind = symbolKind(before, after);

	if (before.nominalType || after.nominalType) {
		return inconclusiveSymbol(
			itemKind,
			"changed nominal TypeScript declaration requires review",
			"private or protected members make cross-snapshot identity comparison inconclusive",
		);
	}

	let additive = false;

	if (before.hasValue || after.hasValue) {
		if (!before.hasValue || !after.hasValue) {
			return breakingSymbol(itemKind, "changed the value/type namespace of public export");
		}

		const beforeValue = checker.getTypeOfSymbolAtLocation(before.symbol, before.declaration);
		const afterValue = checker.getTypeOfSymbolAtLocation(after.symbol, after.declaration);

		if (!checker.isTypeAssignableTo(afterValue, beforeValue)) {
			return breakingSymbol(
				itemKind,
				"new value contract is not assignable to the previous contract for",
			);
		}

		additive = valueAddsCapability(ts, checker, beforeValue, afterValue);
	}

	if (before.hasType || after.hasType) {
		if (!before.hasType || !after.hasType) {
			return breakingSymbol(itemKind, "changed the value/type namespace of public export");
		}

		if (before.genericType || after.genericType) {
			return inconclusiveSymbol(
				itemKind,
				"changed generic TypeScript declaration requires review",
				"generic declaration identity prevents a conclusive cross-snapshot comparison",
			);
		}

		for (const [name, beforeMember] of before.memberShapes) {
			const afterMember = after.memberShapes.get(name);

			if (afterMember === undefined) {
				return breakingSymbol(itemKind, `removed public member ${name} from`);
			}
			if (!beforeMember.readonly && afterMember.readonly) {
				return breakingSymbol(itemKind, `public member ${name} became readonly on`);
			}
			if (beforeMember.optional && !afterMember.optional) {
				return breakingSymbol(itemKind, `public member ${name} became required on`);
			}
			additive = additive || (beforeMember.readonly && !afterMember.readonly);
		}

		const beforeType = checker.getDeclaredTypeOfSymbol(before.symbol);
		const afterType = checker.getDeclaredTypeOfSymbol(after.symbol);
		const beforeToAfter = checker.isTypeAssignableTo(beforeType, afterType);
		const afterToBefore = checker.isTypeAssignableTo(afterType, beforeType);

		if (!beforeToAfter || !afterToBefore) {
			return breakingSymbol(itemKind, "type is not mutually assignable across snapshots for");
		}

		additive = additive || [...after.typeMembers].some((member) => !before.typeMembers.has(member));
	}

	if (additive) {
		return {
			outcome: "additive",
			suggestedBump: "minor",
			itemKind,
			summary: "new value contract adds assignable capability to",
			confidence: "high",
			completeness: "complete",
			coverage: "TypeScript checked value assignability in both directions",
			fallbackReason: null,
		};
	}

	return {
		outcome: "compatible",
		suggestedBump: "none",
		itemKind,
		summary: "declaration remains consumer-compatible for",
		confidence: "high",
		completeness: "complete",
		coverage: "TypeScript checked consumer-visible assignability in both directions",
		fallbackReason: null,
	};
}

function valueAddsCapability(ts, checker, beforeType, afterType) {
	const beforeProperties = new Set(
		checker.getPropertiesOfType(beforeType).map((property) => property.name),
	);
	if (
		checker.getPropertiesOfType(afterType).some((property) => !beforeProperties.has(property.name))
	) {
		return true;
	}

	for (const kind of [ts.SignatureKind.Call, ts.SignatureKind.Construct]) {
		const beforeSignatures = checker.getSignaturesOfType(beforeType, kind);
		const afterSignatures = checker.getSignaturesOfType(afterType, kind);

		if (afterSignatures.length > beforeSignatures.length) {
			return true;
		}
		for (let index = 0; index < beforeSignatures.length; index += 1) {
			const beforeSignature = beforeSignatures[index];
			const afterSignature = afterSignatures[index];

			if (
				afterSignature !== undefined &&
				signatureAddsInputCapability(checker, beforeSignature, afterSignature)
			) {
				return true;
			}
		}
	}

	return false;
}

function signatureAddsInputCapability(checker, before, after) {
	const beforeParameters = before.getParameters();
	const afterParameters = after.getParameters();

	if (afterParameters.length > beforeParameters.length) {
		return true;
	}

	for (let index = 0; index < beforeParameters.length; index += 1) {
		const beforeParameter = beforeParameters[index];
		const afterParameter = afterParameters[index];
		const beforeDeclaration = beforeParameter.valueDeclaration;
		const afterDeclaration = afterParameter?.valueDeclaration;

		if (afterParameter === undefined || afterDeclaration === undefined) {
			continue;
		}
		if (
			beforeDeclaration !== undefined &&
			!checker.isOptionalParameter(beforeDeclaration) &&
			checker.isOptionalParameter(afterDeclaration)
		) {
			return true;
		}

		const beforeType = checker.getTypeOfSymbolAtLocation(
			beforeParameter,
			beforeDeclaration ?? before.getDeclaration(),
		);
		const afterType = checker.getTypeOfSymbolAtLocation(afterParameter, afterDeclaration);
		if (
			checker.isTypeAssignableTo(beforeType, afterType) &&
			!checker.isTypeAssignableTo(afterType, beforeType)
		) {
			return true;
		}
	}

	return false;
}

function symbolKind(before, after) {
	const hasType = before.hasType || after.hasType;
	const hasValue = before.hasValue || after.hasValue;

	if (hasType && hasValue) {
		return "type_and_value";
	}

	return hasType ? "type" : "value";
}

function breakingSymbol(itemKind, summary) {
	return {
		outcome: "breaking",
		suggestedBump: "major",
		itemKind,
		summary,
		confidence: "high",
		completeness: "complete",
		coverage: "TypeScript proved a consumer-visible assignability failure",
		fallbackReason: null,
	};
}

function inconclusiveSymbol(itemKind, summary, reason) {
	return {
		outcome: "inconclusive",
		suggestedBump: "patch",
		itemKind,
		summary,
		confidence: "low",
		completeness: "partial",
		coverage: "the declaration was emitted but could not be compared conclusively",
		fallbackReason: reason,
	};
}

function changeForSymbol(
	ts,
	checker,
	request,
	outcome,
	kind,
	itemPath,
	beforeSymbol,
	afterSymbol,
	declarationPath,
	summary,
) {
	const before = beforeSymbol === null ? null : describeSymbol(ts, checker, beforeSymbol);
	const after = afterSymbol === null ? null : describeSymbol(ts, checker, afterSymbol);
	const description = before ?? after;

	return {
		outcome,
		suggestedBump: bumpForOutcome(outcome),
		kind,
		itemKind: description === null ? "export" : symbolKind(description, description),
		itemPath,
		summary: `${summary} \`${itemPath}\``,
		filePath: relativeDeclarationPath(request, declarationPath),
		beforeSignature: before?.signature ?? null,
		afterSignature: after?.signature ?? null,
		confidence: "high",
		completeness: "complete",
		coverage: "the explicit typed entrypoint and its exports were enumerated",
		fallbackReason: null,
	};
}

function changeForEntrypoint(outcome, kind, itemPath, beforePath, afterPath, summary) {
	return {
		outcome,
		suggestedBump: bumpForOutcome(outcome),
		kind,
		itemKind: "typed_entrypoint",
		itemPath,
		summary: `${summary} \`${itemPath}\``,
		filePath: "package.json",
		beforeSignature: beforePath,
		afterSignature: afterPath,
		confidence: "high",
		completeness: "complete",
		coverage: "explicit typed entrypoints were enumerated from package metadata",
		fallbackReason: null,
	};
}

function inconclusiveChange(itemPath, reason, coverage) {
	return {
		outcome: "inconclusive",
		suggestedBump: "patch",
		kind: "modified",
		itemKind: "declaration_analysis",
		itemPath,
		summary: `TypeScript declaration compatibility is inconclusive for \`${itemPath}\``,
		filePath: "package.json",
		beforeSignature: null,
		afterSignature: null,
		confidence: "low",
		completeness: "partial",
		coverage,
		fallbackReason: reason,
	};
}

function recordComparisonInput(access) {
	access.endpoint.externalInputs.add(access.normalized);
}

function bumpForOutcome(outcome) {
	switch (outcome) {
		case "breaking":
			return "major";
		case "additive":
			return "minor";
		case "compatible":
			return "none";
		default:
			return "patch";
	}
}

function relativeDeclarationPath(request, fileName) {
	for (const side of ["before", "after"]) {
		const packageRoot = path.posix.join(VIRTUAL_ROOT, side, request.packageRoot);

		if (isPathInside(fileName, packageRoot)) {
			return path.posix.relative(packageRoot, fileName);
		}
	}

	return "package.json";
}

function formatDiagnostic(ts, diagnostic) {
	const message = ts
		.flattenDiagnosticMessageText(diagnostic.messageText, " ")
		.replaceAll(/\/__monochange_typescript\/(?:before|after)\//gu, "");

	if (diagnostic.file === undefined || diagnostic.start === undefined) {
		return `TS${diagnostic.code}: ${message}`;
	}

	const position = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);

	return `${path.posix.basename(diagnostic.file.fileName)}:${position.line + 1}:${position.character + 1} TS${diagnostic.code}: ${message}`;
}

function normalizeRelativePath(value) {
	return value
		.replaceAll("\\", "/")
		.replace(/^\.\//u, "")
		.replace(/^\/+|\/+$/gu, "");
}

function normalizeAbsolutePath(value) {
	return path.posix.normalize(value.replaceAll("\\", "/"));
}

function isPathInside(fileName, root) {
	return fileName === root || fileName.startsWith(`${root}/`);
}

function isDeclarationFile(fileName) {
	return /\.d\.(?:c|m)?ts$/u.test(fileName);
}

function isTypeScriptSource(fileName) {
	return /\.(?:c|m)?tsx?$/u.test(fileName) && !isDeclarationFile(fileName);
}

function errorMessage(error) {
	return error instanceof Error ? error.message : String(error);
}
