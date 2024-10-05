import type { CustomThemeConfig } from '@skeletonlabs/tw-plugin'

// https://www.skeleton.dev/docs/generator
// https://dfinity.frontify.com/d/pD7yZhsmpqos/internet-computer#/internet-computer/colors
export const ICPandaTheme: CustomThemeConfig = {
  name: 'icpanda',
  properties: {
    // =~= Theme Properties =~=
    '--theme-font-family-base': `system-ui`,
    '--theme-font-family-heading': `system-ui`,
    '--theme-font-color-base': '0 0 0',
    '--theme-font-color-dark': '255 255 255',
    '--theme-rounded-base': '9999px',
    '--theme-rounded-container': '8px',
    '--theme-border-base': '1px',
    // =~= Theme On-X Colors =~=
    '--on-primary': '255 255 255',
    '--on-secondary': '255 255 255',
    '--on-tertiary': '255 255 255',
    '--on-success': '255 255 255',
    '--on-warning': '255 255 255',
    '--on-error': '255 255 255',
    '--on-surface': '255 255 255',
    // =~= Theme Colors  =~=
    // primary | #11c291
    '--color-primary-50': '219 246 239', // #dbf6ef
    '--color-primary-100': '207 243 233', // #cff3e9
    '--color-primary-200': '196 240 228', // #c4f0e4
    '--color-primary-300': '160 231 211', // #a0e7d3
    '--color-primary-400': '88 212 178', // #58d4b2
    '--color-primary-500': '17 194 145', // #11c291
    '--color-primary-600': '15 175 131', // #0faf83
    '--color-primary-700': '13 146 109', // #0d926d
    '--color-primary-800': '10 116 87', // #0a7457
    '--color-primary-900': '8 95 71', // #085f47
    // secondary | #3b00b9
    '--color-secondary-50': '226 217 245', // #e2d9f5
    '--color-secondary-100': '216 204 241', // #d8ccf1
    '--color-secondary-200': '206 191 238', // #cebfee
    '--color-secondary-300': '177 153 227', // #b199e3
    '--color-secondary-400': '118 77 206', // #764dce
    '--color-secondary-500': '59 0 185', // #3b00b9
    '--color-secondary-600': '53 0 167', // #3500a7
    '--color-secondary-700': '44 0 139', // #2c008b
    '--color-secondary-800': '35 0 111', // #23006f
    '--color-secondary-900': '29 0 91', // #1d005b
    // tertiary | #29abe2
    '--color-tertiary-50': '223 242 251', // #dff2fb
    '--color-tertiary-100': '212 238 249', // #d4eef9
    '--color-tertiary-200': '202 234 248', // #caeaf8
    '--color-tertiary-300': '169 221 243', // #a9ddf3
    '--color-tertiary-400': '105 196 235', // #69c4eb
    '--color-tertiary-500': '41 171 226', // #29abe2
    '--color-tertiary-600': '37 154 203', // #259acb
    '--color-tertiary-700': '31 128 170', // #1f80aa
    '--color-tertiary-800': '25 103 136', // #196788
    '--color-tertiary-900': '20 84 111', // #14546f
    // success | #84cc16
    '--color-success-50': '237 247 220', // #edf7dc
    '--color-success-100': '230 245 208', // #e6f5d0
    '--color-success-200': '224 242 197', // #e0f2c5
    '--color-success-300': '206 235 162', // #ceeba2
    '--color-success-400': '169 219 92', // #a9db5c
    '--color-success-500': '132 204 22', // #84cc16
    '--color-success-600': '119 184 20', // #77b814
    '--color-success-700': '99 153 17', // #639911
    '--color-success-800': '79 122 13', // #4f7a0d
    '--color-success-900': '65 100 11', // #41640b
    // warning | #fbb03b
    '--color-warning-50': '254 243 226', // #fef3e2
    '--color-warning-100': '254 239 216', // #feefd8
    '--color-warning-200': '254 235 206', // #feebce
    '--color-warning-300': '253 223 177', // #fddfb1
    '--color-warning-400': '252 200 118', // #fcc876
    '--color-warning-500': '251 176 59', // #fbb03b
    '--color-warning-600': '226 158 53', // #e29e35
    '--color-warning-700': '188 132 44', // #bc842c
    '--color-warning-800': '151 106 35', // #976a23
    '--color-warning-900': '123 86 29', // #7b561d
    // error | #ed1e79
    '--color-error-50': '252 221 235', // #fcddeb
    '--color-error-100': '251 210 228', // #fbd2e4
    '--color-error-200': '251 199 222', // #fbc7de
    '--color-error-300': '248 165 201', // #f8a5c9
    '--color-error-400': '242 98 161', // #f262a1
    '--color-error-500': '237 30 121', // #ed1e79
    '--color-error-600': '213 27 109', // #d51b6d
    '--color-error-700': '178 23 91', // #b2175b
    '--color-error-800': '142 18 73', // #8e1249
    '--color-error-900': '116 15 59', // #740f3b
    // surface | #737373
    '--color-surface-50': '234 234 234', // #eaeaea
    '--color-surface-100': '227 227 227', // #e3e3e3
    '--color-surface-200': '220 220 220', // #dcdcdc
    '--color-surface-300': '199 199 199', // #c7c7c7
    '--color-surface-400': '157 157 157', // #9d9d9d
    '--color-surface-500': '115 115 115', // #737373
    '--color-surface-600': '104 104 104', // #686868
    '--color-surface-700': '86 86 86', // #565656
    '--color-surface-800': '69 69 69', // #454545
    '--color-surface-900': '56 56 56' // #383838
  }
}
