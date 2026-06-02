use super::*;

/// 测试 strip_leaked_prepends：有原始历史时，通过 ID 匹配定位并剥离 leaked system prepends
#[test]
fn test_strip_leaked_prepends_有历史时剥离头部system消息() {
    // Arrange: 原始历史 [Human("hello"), Ai("hi")]
    let history = [BaseMessage::human("hello"), BaseMessage::ai("hi")];
    // 模拟 execute() 错误路径返回的 messages:
    // [SystemPrepend, SystemPrompt, Human("hello"), Ai("hi"), Human("new"), Ai("response")]
    let leaked_system_1 = BaseMessage::system("injected by middleware");
    let leaked_system_2 = BaseMessage::system("system prompt");
    let result_messages = vec![
        leaked_system_1,
        leaked_system_2,
        history[0].clone(),
        history[1].clone(),
        BaseMessage::human("new question"),
        BaseMessage::ai("response"),
    ];
    // Act
    let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));
    // Assert: 应该去掉头部两条 leaked system，保留从原始历史开始的所有消息
    assert_eq!(cleaned.len(), 4, "应去掉2条leaked system，剩4条");
    assert_eq!(
        cleaned[0].id(),
        history[0].id(),
        "第一条应为原始历史的第一条"
    );
    assert!(!cleaned[0].is_system(), "不应包含leaked system");
}

/// 测试 strip_leaked_prepends：原始历史为空时，剥离所有头部 system 消息
#[test]
fn test_strip_leaked_prepends_空历史时剥离头部system() {
    // Arrange: 空历史
    let history: Vec<BaseMessage> = vec![];
    let result_messages = vec![
        BaseMessage::system("injected by middleware"),
        BaseMessage::system("system prompt"),
        BaseMessage::human("new question"),
        BaseMessage::ai("response"),
    ];
    // Act
    let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));
    // Assert: 应该去掉头部两条 system，只保留 human + ai
    assert_eq!(cleaned.len(), 2, "应去掉2条leaked system，剩2条");
    assert!(!cleaned[0].is_system(), "第一条不应是system消息");
}

/// 测试 strip_leaked_prepends：原始历史在 result 中找不到（compact 替换场景）
#[test]
fn test_strip_leaked_prepends_历史id找不到时原样返回() {
    // Arrange: 原始历史有一条消息
    let history = [BaseMessage::human("hello")];
    // result_messages 中不包含原始历史的消息（compact 替换了所有消息）
    let result_messages = vec![
        BaseMessage::system("system prompt"),
        BaseMessage::human("compacted summary"),
        BaseMessage::ai("response"),
    ];
    // Act
    let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));
    // Assert: 找不到原始历史，原样返回
    assert_eq!(cleaned.len(), 3, "找不到原始历史时应原样返回");
}

/// 测试 strip_leaked_prepends：没有 leaked prepends 时正常返回
#[test]
fn test_strip_leaked_prepends_无leaked时正常返回() {
    let history = [BaseMessage::human("hello"), BaseMessage::ai("hi")];
    // 没有 leaked system，直接是原始历史 + 新消息
    let result_messages = vec![
        history[0].clone(),
        history[1].clone(),
        BaseMessage::human("new question"),
    ];
    let cleaned = strip_leaked_prepends(&result_messages, history.first().map(|m| m.id()));
    assert_eq!(cleaned.len(), 3, "无leaked时应正常返回所有消息");
    assert_eq!(cleaned[0].id(), history[0].id());
}
