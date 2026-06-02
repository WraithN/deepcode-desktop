import { useRef } from 'react';
import { FolderOpen } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

interface DirectoryPickerButtonProps {
  onSelect: (path: string) => void;
}

export default function DirectoryPickerButton({ onSelect }: DirectoryPickerButtonProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  const handleClick = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        onSelect(selected);
      }
    } catch {
      // Tauri dialog 不可用，降级到原生文件选择
      inputRef.current?.click();
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      // 取第一个文件的路径，提取目录部分
      const filePath = files[0].webkitRelativePath || files[0].name;
      const dirPath = filePath.includes('/') ? filePath.split('/')[0] : '.';
      onSelect(dirPath);
    }
    // 重置 input 以便可以重复选择同一目录
    if (inputRef.current) inputRef.current.value = '';
  };

  return (
    <>
      <button
        type="button"
        onClick={handleClick}
        className="w-7 h-7 flex items-center justify-center rounded border border-border bg-secondary text-muted-foreground hover:text-foreground hover:bg-secondary/80 transition-colors shrink-0"
        title="选择文件夹"
      >
        <FolderOpen className="w-3.5 h-3.5" />
      </button>
      <input
        ref={inputRef}
        type="file"
        {...{ webkitdirectory: 'true', directory: '' }}
        onChange={handleChange}
        className="hidden"
      />
    </>
  );
}
