import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback: ReactNode;
  /** 可选：错误发生时记录日志，默认用 console.error */
  onError?: (error: Error, info: ErrorInfo) => void;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * React Error Boundary — 捕获子树中渲染阶段的异常，防止白屏。
 *
 * 使用方式：
 *   <ErrorBoundary fallback={<p>出错了</p>}>
 *     <RiskyComponent />
 *   </ErrorBoundary>
 *
 * 设计要点：
 *   1. class 组件是必须的 — React 至今没有 hooks 版本的 Error Boundary
 *   2. getDerivedStateFromError 切换降级 UI
 *   3. componentDidCatch 记录错误日志
 *   4. 不捕获事件处理器/异步代码中的异常（那些用 try/catch）
 */
export default class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    if (this.props.onError) {
      this.props.onError(error, info);
    } else {
      console.error(
        `[ErrorBoundary] ${error.message}`,
        "\n组件栈:",
        info.componentStack,
      );
    }
  }

  /** 重置错误状态（外部可通过 key 变化触发重试） */
  reset = () => {
    this.setState({ hasError: false, error: null });
  };

  render(): ReactNode {
    if (this.state.hasError) {
      return this.props.fallback;
    }
    return this.props.children;
  }
}
