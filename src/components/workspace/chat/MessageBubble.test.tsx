import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import MessageBubble from './MessageBubble';
import type { Message } from '@/types/types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('MessageBubble', () => {
  const createMessage = (overrides: Partial<Message> = {}): Message => ({
    id: 'msg-1',
    conversation_id: 'conv-1',
    role: 'assistant',
    content: 'Hello world',
    created_at: new Date().toISOString(),
    is_complete: true,
    ...overrides,
  });

  it('renders user message with correct text', () => {
    render(<MessageBubble message={createMessage({ role: 'user', content: 'User message' })} />);
    expect(screen.getByText('User message')).toBeInTheDocument();
    expect(screen.getByText('你')).toBeInTheDocument();
  });

  it('renders assistant message with correct text', () => {
    render(<MessageBubble message={createMessage({ role: 'assistant', content: 'AI response' })} />);
    expect(screen.getByText('AI response')).toBeInTheDocument();
    expect(screen.getByText('AI助手')).toBeInTheDocument();
  });

  it('shows complete status for assistant message', () => {
    render(<MessageBubble message={createMessage({ is_complete: true })} />);
    expect(screen.getByText('编程已完成')).toBeInTheDocument();
  });

  it('shows in-progress status for incomplete assistant message', () => {
    render(<MessageBubble message={createMessage({ is_complete: false, content: '' })} />);
    expect(screen.getByText('进行中')).toBeInTheDocument();
  });

  it('calls onEditUserMessage when edit button is clicked', () => {
    const onEdit = vi.fn();
    render(
      <MessageBubble
        message={createMessage({ role: 'user', content: 'Edit me' })}
        onEditUserMessage={onEdit}
      />,
    );
    const editButton = screen.getByText('编辑');
    fireEvent.click(editButton);
    expect(onEdit).toHaveBeenCalledWith('Edit me');
  });

  it('renders code blocks in message content', () => {
    const content = 'Here is code:\n```tsx\nconst x = 1;\n```\nDone.';
    render(<MessageBubble message={createMessage({ content })} />);
    expect(screen.getByText('const x = 1;')).toBeInTheDocument();
  });

  it('displays token stats when available', () => {
    render(
      <MessageBubble
        message={createMessage({
          token_in: 100,
          token_out: 50,
          duration_ms: 2500,
        })}
      />,
    );
    expect(screen.getByText('输入 100 tokens')).toBeInTheDocument();
    expect(screen.getByText('输出 50 tokens')).toBeInTheDocument();
    expect(screen.getByText('2.5s')).toBeInTheDocument();
  });
});
