import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ModeSelector, ModelSelector } from './ChatSelectors';

describe('ChatSelectors', () => {
  describe('ModeSelector', () => {
    it('renders current mode label', () => {
      render(<ModeSelector value="plan" onChange={vi.fn()} />);
      expect(screen.getByText('Plan')).toBeInTheDocument();
    });

    it('switches to build mode when clicked', () => {
      const onChange = vi.fn();
      render(<ModeSelector value="plan" onChange={onChange} />);
      fireEvent.click(screen.getByText('Plan'));
      fireEvent.click(screen.getByText('Build'));
      expect(onChange).toHaveBeenCalledWith('build');
    });

    it('switches to plan mode when clicked', () => {
      const onChange = vi.fn();
      render(<ModeSelector value="build" onChange={onChange} />);
      fireEvent.click(screen.getByText('Build'));
      fireEvent.click(screen.getByText('Plan'));
      expect(onChange).toHaveBeenCalledWith('plan');
    });
  });

  describe('ModelSelector', () => {
    it('renders current model label', () => {
      render(<ModelSelector value="gpt-4" onChange={vi.fn()} />);
      expect(screen.getByText('GPT-4')).toBeInTheDocument();
    });

    it('switches model when clicked', () => {
      const onChange = vi.fn();
      render(<ModelSelector value="gpt-4" onChange={onChange} />);
      fireEvent.click(screen.getByText('GPT-4'));
      fireEvent.click(screen.getByText('Claude 3 Opus'));
      expect(onChange).toHaveBeenCalledWith('claude-3-opus');
    });

    it('renders fallback value when model not in list', () => {
      render(<ModelSelector value="custom-model" onChange={vi.fn()} />);
      expect(screen.getByText('custom-model')).toBeInTheDocument();
    });
  });
});
