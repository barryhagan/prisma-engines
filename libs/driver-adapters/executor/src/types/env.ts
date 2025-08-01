import * as S from '@effect/schema/Schema'

const DriverAdapterConfig = S.struct({
  proxy_url: S.string.pipe(
    S.nonEmpty({
      message: () => '`proxy_url` must not be empty',
    }),
  ),
})

const DriverAdapterConfigFromString = S.transform(
  S.string,
  DriverAdapterConfig,
  (str) => JSON.parse(str),
  (config) => JSON.stringify(config),
)

const EnvPlanetScale = S.struct({
  DRIVER_ADAPTER: S.literal('planetscale'),
  DRIVER_ADAPTER_CONFIG: S.union(
    DriverAdapterConfigFromString,
    DriverAdapterConfig,
  ),
})

const EnvNeonWS = S.struct({
  DRIVER_ADAPTER: S.literal('neon:ws'),
  DRIVER_ADAPTER_CONFIG: S.union(
    DriverAdapterConfigFromString,
    DriverAdapterConfig,
  ),
})

export const MobileAdapterConfig = S.struct({
  EXTERNAL_TEST_EXECUTOR: S.literal('Mobile'),
  MOBILE_EMULATOR_URL: S.string.pipe(
    S.nonEmpty({
      message: () => '`MOBILE_EMULATOR_URL` must not be empty',
    }),
  ),
})

export const ExternalTestExecutor = S.literal('Wasm', 'Napi', 'QueryCompiler')

export const Env = S.extend(
  S.extend(
    S.union(
      EnvPlanetScale,
      EnvNeonWS,
      S.struct({
        DRIVER_ADAPTER: S.literal(
          'pg',
          'libsql',
          'd1',
          'better-sqlite3',
          'mssql',
          'mariadb',
        ),
      }),
    ),
    S.union(
      MobileAdapterConfig,
      S.struct({
        EXTERNAL_TEST_EXECUTOR: S.optional(ExternalTestExecutor, {
          default: () => 'Napi',
        }),
      }),
    ),
  ),
  S.struct({
    CONNECTOR: S.literal(
      'postgres',
      'cockroachdb',
      'sqlite',
      'mysql',
      'sqlserver',
      'vitess',
    ),
  }),
)

export type Env = S.Schema.Type<typeof Env>

export type DriverAdapterTag = Env['DRIVER_ADAPTER']

export type EnvForAdapter<T extends DriverAdapterTag> = Env & {
  readonly DRIVER_ADAPTER: T
}
