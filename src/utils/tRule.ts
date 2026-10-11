import { TFunction } from 'i18next';

interface RuleTrans {
  title: string;
  message: string;
}

export function tRule(
  t: TFunction,
  ruleId: string,
  vars: Record<string, string> = {},
): RuleTrans {
  const title = t(`rule.${ruleId}.title`, { defaultValue: ruleId });
  const message = t(`rule.${ruleId}.message`, {
    defaultValue: '',
    ...vars,
  });
  return { title, message };
}
