use raider_opencode::types::question::QuestionRequest;

pub fn question_to_prompt(req: &QuestionRequest) -> raider_tui::QuestionPrompt {
    use raider_tui::{QuestionInfo as TuiInfo, QuestionOption as TuiOption, QuestionPrompt};
    let questions: Vec<TuiInfo> = req
        .questions
        .iter()
        .map(|q| TuiInfo {
            question: q.question.clone(),
            header: q.header.clone(),
            options: q
                .options
                .iter()
                .map(|o| TuiOption {
                    label: o.label.clone(),
                    description: o.description.clone(),
                })
                .collect(),
            multiple: q.is_multiple(),
            custom_allowed: q.allows_custom(),
        })
        .collect();
    QuestionPrompt::new(req.id.clone(), req.session_id.as_str(), questions)
}
