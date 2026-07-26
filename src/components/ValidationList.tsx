import type { ValidationReport } from "../types";
import { severityText } from "../lib/labels";
import { EmptyState } from "./ui";

export function ValidationList({ validation }: { validation: ValidationReport | null }) {
  if (!validation) {
    return <EmptyState title="等待检查" text="创建项目或修改参数后，AutoMD 会自动检查是否缺少必须处理的问题。" />;
  }
  const summaryText: Record<ValidationReport["status"], { title: string; text: string }> = {
    valid: {
      title: "参数检查通过",
      text: "暂未发现必须处理的问题，可以继续生成结构准备文件或运行包。"
    },
    validWithWarnings: {
      title: "有提示需要阅读",
      text: "可以继续，但建议先看下面的 warning，确认它们符合你的体系和引擎选择。"
    },
    invalid: {
      title: "需要先修正参数",
      text: "存在 error 时不要运行；先按下面的字段和说明修改参数或结构输入。"
    }
  };
  const summary = summaryText[validation.status];
  return (
    <div className="validation-list">
      <div className={`validation-summary ${validation.status}`}>
        <strong>{summary.title}</strong>
        <span>{validation.items.length ? `${validation.items.length} 条提示` : summary.text}</span>
      </div>
      {validation.items.map((item, index) => (
        <div className={`validation-item ${item.severity}`} key={`${item.field}-${index}`}>
          <span>{severityText[item.severity]}</span>
          <strong>{item.field}</strong>
          <p>{item.message}</p>
        </div>
      ))}
    </div>
  );
}

