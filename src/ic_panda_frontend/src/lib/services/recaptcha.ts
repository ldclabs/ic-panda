/// <reference types="@types/grecaptcha" />
import { GOOGLE_RECAPTCHA_ID } from '$lib/constants'

export async function getReCaptcha(): Promise<ReCaptchaV2.ReCaptcha> {
  return new Promise((resolve, reject) => {
    if (!window.grecaptcha) {
      return reject(new Error('Google reCAPTCHA not loaded'))
    }

    const timer = setTimeout(() => {
      reject(new Error('Google reCAPTCHA load failed'))
    }, 5000)

    grecaptcha.enterprise.ready(() => {
      clearTimeout(timer)
      resolve(grecaptcha.enterprise)
    })
  })
}

// Returns the token from the reCAPTCHA
export async function executeReCaptcha(action: string): Promise<string> {
  const recaptcha = await getReCaptcha()
  return recaptcha.execute(GOOGLE_RECAPTCHA_ID, { action })
}
