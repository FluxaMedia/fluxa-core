use super::*;

pub(super) fn route_intro_segments(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the data JSON for single-arg methods
        "parseIntroDbSegments" => opt_json(intro_segments::parse_intro_db_segments_json(args_json)),
        "parseSkipdbSegments" => opt_json(intro_segments::parse_skipdb_segments_json(args_json)),
        "anilistMalId" => opt_json(intro_segments::anilist_mal_id_json(args_json)),
        "parseAniskipResults" => opt_json(intro_segments::parse_aniskip_results_json(args_json)),
        "parseAnimeSkipResults" => {
            opt_json(intro_segments::parse_anime_skip_results_json(args_json))
        }
        "uniqueIntroSegments" => {
            let args = object(args_json)?;
            opt_json(intro_segments::unique_intro_segments_json(
                field_str(&args, "segmentsAJson")?,
                field_str(&args, "segmentsBJson")?,
            ))
        }
        "mergeIntroSegments" => opt_json(intro_segments::merge_intro_segments_json(args_json)),
        "matchAnimeSkipEpisodeId" => {
            let args = object(args_json)?;
            let season = field(&args, "season")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "season must be a number"))?;
            let episode = field(&args, "episode")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "episode must be a number"))?;
            opt_json(
                intro_segments::match_anime_skip_episode_id(
                    field_str(&args, "episodesJson")?,
                    season,
                    episode,
                )
                .and_then(|id| serde_json::to_string(&id).ok()),
            )
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

pub(super) fn route_plugins(method: &str, args_json: &str) -> Outcome {
    match method {
        "pluginManifestParse" => {
            let normalized = plugins::parse_plugin_manifest_json(args_json)
                .map_err(|message| fail(ErrorKind::InvalidArgs, message))?;
            into_json(normalized)
        }
        "pluginExecutionPlan" => opt_json(plugins::plugin_execution_plan_json(args_json)),
        "pluginStreamResultsParse" => {
            into_json(plugins::parse_plugin_stream_results_json(args_json))
        }
        "pluginStreamResultsToStreams" => {
            into_json(plugins::plugin_stream_results_to_streams_json(args_json))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}
