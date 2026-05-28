declare module 'react-file-icon' {
  import type { CSSProperties, SVGProps } from 'react'

  export type FileIconProps = SVGProps<SVGSVGElement> & {
    extension?: string
    labelColor?: string
    glyphColor?: string
    foldColor?: string
    gradientColor?: string
    radius?: number
    typeColor?: string
  }

  export const FileIcon: (props: FileIconProps) => JSX.Element

  export const defaultStyles: Record<string, Partial<FileIconProps> & { color?: CSSProperties['color'] }>
}
