export function parseOptions(args) {
  const options = { browser: 'chromium', help: false }
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index]
    if (argument === '--help') {
      options.help = true
      continue
    }
    if (!['--manifest', '--out', '--browser'].includes(argument)) throw new Error(`unknown argument: ${argument}`)
    const key = argument.slice(2)
    if (Object.hasOwn(options, key) && key !== 'browser') throw new Error(`duplicate argument: ${argument}`)
    if (key === 'browser' && args.indexOf(argument) !== index) throw new Error(`duplicate argument: ${argument}`)
    const value = args[++index]
    if (!value || value.startsWith('--')) throw new Error(`missing value for ${argument}`)
    options[key] = value
  }
  if (!options.help && (!options.manifest || !options.out)) throw new Error('--manifest and --out are required; see --help')
  if (!['chromium', 'webkit', 'firefox'].includes(options.browser)) throw new Error(`unsupported browser: ${options.browser}`)
  return options
}

/** Cleanup failures remain reportable and cannot prevent later resources from closing. */
export async function closeResource(name, resource, errors) {
  if (!resource) return
  try {
    await resource.close()
  } catch (error) {
    errors.push(`${name} cleanup failed: ${String(error)}`)
  }
}
