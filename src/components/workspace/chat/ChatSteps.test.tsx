import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StepItem, ReadFileContent, PermissionStep } from './ChatSteps';
import type { MessageStep } from '@/types/types';

describe('ChatSteps', () => {
  describe('ReadFileContent', () => {
    it('renders plain text', () => {
      render(<ReadFileContent content="Hello world" />);
      expect(screen.getByText('Hello world')).toBeInTheDocument();
    });

    it('renders code blocks', () => {
      render(<ReadFileContent content="```tsx\nconst x = 1;\n```" />);
      // Code content is rendered inside <pre><code>
      expect(document.querySelector('pre')).toBeInTheDocument();
    });

    it('renders inline code', () => {
      render(<ReadFileContent content="Use `console.log` to debug" />);
      // Inline code is rendered via dangerouslySetInnerHTML
      expect(document.querySelector('code')).toBeInTheDocument();
    });
  });

  describe('StepItem', () => {
    const createStep = (overrides: Partial<MessageStep> = {}): MessageStep => ({
      type: 'thinking',
      content: 'Thinking...',
      ...overrides,
    });

    it('renders thinking step', () => {
      render(<StepItem step={createStep({ type: 'thinking' })} index={0} />);
      expect(screen.getByText('思考中')).toBeInTheDocument();
    });

    it('renders tool_use step', () => {
      render(<StepItem step={createStep({ type: 'tool_use', content: 'Using tool' })} index={0} />);
      expect(screen.getByText('使用工具')).toBeInTheDocument();
    });

    it('renders tool_result step', () => {
      render(<StepItem step={createStep({ type: 'tool_result' })} index={0} />);
      expect(screen.getByText('工具结果')).toBeInTheDocument();
    });

    it('renders final step', () => {
      render(<StepItem step={createStep({ type: 'final' })} index={0} />);
      expect(screen.getByText('结果')).toBeInTheDocument();
    });

    it('calls onRetry when retry button clicked', () => {
      const onRetry = vi.fn();
      render(<StepItem step={createStep({ type: 'tool_use', failed: true })} index={2} onRetry={onRetry} />);
      // Expand the step first
      fireEvent.click(screen.getByText('展开'));
      const retryButton = screen.getByText('点击重试');
      fireEvent.click(retryButton);
      expect(onRetry).toHaveBeenCalled();
    });
  });

  describe('PermissionStep', () => {
    const step: MessageStep = {
      type: 'ask_permission',
      content: 'Allow access?',
      permissionType: 'file_write',
    };

    it('renders permission step with tool name', () => {
      render(<PermissionStep step={step} onAnswer={vi.fn()} />);
      expect(screen.getByText('权限询问 · file_write')).toBeInTheDocument();
    });

    it('calls onAnswer with once when first button clicked', () => {
      const onAnswer = vi.fn();
      render(<PermissionStep step={step} onAnswer={onAnswer} />);
      fireEvent.click(screen.getByText('本次同意 (once)'));
      expect(onAnswer).toHaveBeenCalledWith('once');
    });

    it('calls onAnswer with session when second button clicked', () => {
      const onAnswer = vi.fn();
      render(<PermissionStep step={step} onAnswer={onAnswer} />);
      fireEvent.click(screen.getByText('本 Session 同意 (always)'));
      expect(onAnswer).toHaveBeenCalledWith('session');
    });

    it('calls onAnswer with deny when third button clicked', () => {
      const onAnswer = vi.fn();
      render(<PermissionStep step={step} onAnswer={onAnswer} />);
      fireEvent.click(screen.getByText('不同意 (reject)'));
      expect(onAnswer).toHaveBeenCalledWith('deny');
    });
  });
});
