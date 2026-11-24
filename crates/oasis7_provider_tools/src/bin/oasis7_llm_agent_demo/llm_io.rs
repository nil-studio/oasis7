use oasis7_simulator::AgentDecisionTrace;

pub(crate) fn truncate_for_llm_io_log(text: &str, max_chars: Option<usize>) -> String {
    let Some(max_chars) = max_chars else {
        return text.to_string();
    };
    if max_chars == 0 {
        return text.to_string();
    }

    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    let mut truncated = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            break;
        }
        truncated.push(ch);
    }
    truncated.push_str(&format!(
        "\n...(truncated, total_chars={total_chars}, max_chars={max_chars})"
    ));
    truncated
}

pub(crate) fn print_llm_io_trace(
    tick: u64,
    agent_id: &str,
    trace: &AgentDecisionTrace,
    llm_io_max_chars: Option<usize>,
) {
    println!("tick={} agent={} llm_io_begin", tick, agent_id);

    if let Some(input) = trace.llm_input.as_ref() {
        println!("tick={} agent={} llm_input_begin", tick, agent_id);
        println!("{}", truncate_for_llm_io_log(input, llm_io_max_chars));
        println!("tick={} agent={} llm_input_end", tick, agent_id);
    } else {
        println!("tick={} agent={} llm_input=<none>", tick, agent_id);
    }

    if let Some(output) = trace.llm_output.as_ref() {
        println!("tick={} agent={} llm_output_begin", tick, agent_id);
        println!("{}", truncate_for_llm_io_log(output, llm_io_max_chars));
        println!("tick={} agent={} llm_output_end", tick, agent_id);
    } else {
        println!("tick={} agent={} llm_output=<none>", tick, agent_id);
    }

    if let Some(error) = trace.llm_error.as_ref() {
        println!("tick={} agent={} llm_error={}", tick, agent_id, error);
    }
    if let Some(parse_error) = trace.parse_error.as_ref() {
        println!(
            "tick={} agent={} parse_error={}",
            tick, agent_id, parse_error
        );
    }

    println!("tick={} agent={} llm_io_end", tick, agent_id);
}
