use indoc::indoc;
use query_engine_tests::*;

#[test_suite(schema(common_nullable_types), only(Postgres))]
mod casts {
    use query_engine_tests::{RawParam, fmt_query_raw, run_query};

    // The following tests are excluded for driver adapters. The underlying
    // driver rejects queries where the values of the positional arguments do
    // not match the expected types. As an example, the following query to the
    // driver
    //
    // ```json
    // {
    // sql: 'SELECT $1::int4 AS decimal_to_i4;                         ',
    // args: [ 42.51 ]
    // }
    //
    // Bails with: ERROR: invalid input syntax for type integer: "42.51".
    // It fails in the following cases:
    // - RawParam::from(42.51)
    // - RawParam::decimal("42.51")
    //
    #[connector_test(only(Postgres), exclude(Postgres("neon.js.wasm", "pg.js.wasm")))]
    async fn query_numeric_casts(runner: Runner) -> TestResult<()> {
        insta::assert_snapshot!(
          run_query_pretty!(&runner, fmt_query_raw(r#"
            SELECT
                $1::float4     AS i8_to_f4,
                $2::float8     AS i8_to_f8,

                $3::int4       AS numeric_to_i4,
                $4::int8       AS numeric_to_i8,

                $5::int4       AS bigint_to_i4,
                $6::float4     AS bigint_to_f4,
                $7::float8     AS bigint_to_f8,

                $8::int4       AS decimal_to_i4,
                $9::int8       AS decimal_to_i8,
                $10::float4    AS decimal_to_f4,
                $11::float8    AS decimal_to_f8,

                $12::int4      AS text_to_i4,
                $13::int8      AS text_to_i8,
                $14::float4    AS text_to_f4,
                $15::float8    AS text_to_f8;
            "#,
            vec![
                RawParam::from(42),                // $1
                RawParam::from(42),                // $2
                RawParam::from(42.51),             // $3
                RawParam::from(42.51),             // $4

                RawParam::bigint(42), // $5
                RawParam::bigint(42), // $6
                RawParam::bigint(42), // $7

                RawParam::decimal("42.51"),        // $8
                RawParam::decimal("42.51"),        // $9
                RawParam::decimal("42.51"),        // $10
                RawParam::decimal("42.51"),        // $11

                RawParam::from("42"),              // $12
                RawParam::from("42"),              // $13
                RawParam::from("42.51"),           // $14
                RawParam::from("42.51"),           // $15
            ])),
          @r###"
        {
          "data": {
            "queryRaw": {
              "columns": [
                "i8_to_f4",
                "i8_to_f8",
                "numeric_to_i4",
                "numeric_to_i8",
                "bigint_to_i4",
                "bigint_to_f4",
                "bigint_to_f8",
                "decimal_to_i4",
                "decimal_to_i8",
                "decimal_to_f4",
                "decimal_to_f8",
                "text_to_i4",
                "text_to_i8",
                "text_to_f4",
                "text_to_f8"
              ],
              "types": [
                "float",
                "double",
                "int",
                "bigint",
                "int",
                "float",
                "double",
                "int",
                "bigint",
                "float",
                "double",
                "int",
                "bigint",
                "float",
                "double"
              ],
              "rows": [
                [
                  42.0,
                  42.0,
                  43,
                  "43",
                  42,
                  42.0,
                  42.0,
                  43,
                  "43",
                  42.51,
                  42.51,
                  42,
                  "42",
                  42.51,
                  42.51
                ]
              ]
            }
          }
        }
        "###
        );

        Ok(())
    }

    #[connector_test]
    async fn query_date_casts(runner: Runner) -> TestResult<()> {
        insta::assert_snapshot!(
          run_query_pretty!(&runner, fmt_query_raw(r#"
                SELECT
                    ($1::timestamp - $2::interval)   AS text_to_interval,
                    $3 = DATE_PART('year', $4::date) AS is_year_2023,
                    $5::time                         AS text_to_time
                ;
            "#,
            vec![
                RawParam::from("2022-01-01 00:00:00"), // $1
                RawParam::from("1 year"),              // $2
                RawParam::from(2022),                  // $3
                RawParam::from("2022-01-01"),          // $4,
                RawParam::from("12:34")                // $5
            ])),
          @r###"
        {
          "data": {
            "queryRaw": {
              "columns": [
                "text_to_interval",
                "is_year_2023",
                "text_to_time"
              ],
              "types": [
                "datetime",
                "bool",
                "time"
              ],
              "rows": [
                [
                  "2021-01-01T00:00:00+00:00",
                  true,
                  "12:34:00"
                ]
              ]
            }
          }
        }
        "###
        );

        Ok(())
    }

    fn schema_9949() -> String {
        let schema = indoc! {
            r#"model Employee {
                #id(id, Int, @id, @default(autoincrement()))
                title        String   @test.VarChar(255)
                salary       Decimal  @test.Decimal(8, 2)
                fte          Float?   @test.DoublePrecision
                fteAlternate Float?   @test.Real
              }"#
        };

        schema.to_owned()
    }

    #[connector_test(schema(schema_9949))]
    async fn prisma_9949(runner: Runner) -> TestResult<()> {
        insta::assert_snapshot!(
          run_query!(
              &runner,
              fmt_execute_raw(
                  r#"INSERT INTO casts_prisma_9949."Employee" (title, salary, fte, "fteAlternate") VALUES ($1, $2, $3, $4);"#,
                  vec![
                      RawParam::from("Test Person Number 1"), // $1
                      RawParam::from(45000),                  // $2
                      RawParam::from(1),                      // $3
                      RawParam::from(1),                      // $4
                  ]
              )
          ),
          @r###"{"data":{"executeRaw":1}}"###
        );

        insta::assert_snapshot!(
          run_query!(&runner, r#"{ findManyEmployee { title, salary, fte, fteAlternate } }"#),
          @r###"{"data":{"findManyEmployee":[{"title":"Test Person Number 1","salary":"45000","fte":1,"fteAlternate":1}]}}"###
        );

        Ok(())
    }
}
