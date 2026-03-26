use std::{env, fs};

use zed_extension_api::{
    self as zed, Architecture, DownloadedFileType, LanguageServerId, Os, Result,
};

const LANGUAGE_SERVER_BINARY_STEM: &str = "urdf-language-server";
const GITHUB_REPOSITORY: &str = "MorningFrog/zed-urdf";

/// 必须与 GitHub Release tag 一一对应：v0.1.0 / v0.1.1 / ...
const EXTENSION_VERSION: &str = env!("CARGO_PKG_VERSION");
const LANGUAGE_SERVER_RELEASE_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// 扩展工作目录下的缓存根目录
const DOWNLOAD_ROOT_DIR: &str = ".zed-urdf";

struct PlatformDescriptor {
    target: &'static str,
    archive_ext: &'static str,
    download_type: DownloadedFileType,
    binary_name: &'static str,
    make_executable: bool,
}

fn platform_descriptor(os: Os, arch: Architecture) -> Option<PlatformDescriptor> {
    match (os, arch) {
        (Os::Mac, Architecture::Aarch64) => Some(PlatformDescriptor {
            target: "aarch64-apple-darwin",
            archive_ext: "tar.gz",
            download_type: DownloadedFileType::GzipTar,
            binary_name: "urdf-language-server",
            make_executable: true,
        }),
        (Os::Linux, Architecture::X8664) => Some(PlatformDescriptor {
            target: "x86_64-unknown-linux-musl",
            archive_ext: "tar.gz",
            download_type: DownloadedFileType::GzipTar,
            binary_name: "urdf-language-server",
            make_executable: true,
        }),
        (Os::Windows, Architecture::X8664) => Some(PlatformDescriptor {
            target: "x86_64-pc-windows-msvc",
            archive_ext: "zip",
            download_type: DownloadedFileType::Zip,
            binary_name: "urdf-language-server.exe",
            make_executable: false,
        }),
        _ => None,
    }
}

struct UrdfExtension {
    cached_binary_path: Option<String>,
}

impl UrdfExtension {
    fn versioned_binary_paths(platform: &PlatformDescriptor) -> Result<(String, String)> {
        let version_dir = format!("{DOWNLOAD_ROOT_DIR}/{EXTENSION_VERSION}");
        let binary_rel_path = format!("{version_dir}/{}", platform.binary_name);

        let binary_abs_path = env::current_dir()
            .map_err(|err| format!("failed to resolve extension working directory: {err}"))?
            .join(&binary_rel_path)
            .to_string_lossy()
            .to_string();

        Ok((binary_rel_path, binary_abs_path))
    }

    fn ensure_same_version_language_server(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> Result<String> {
        let (os, arch) = zed::current_platform();
        let Some(platform) = platform_descriptor(os, arch) else {
            return Err(format!("unsupported platform: {:?} / {:?}", os, arch));
        };

        let (binary_rel_path, binary_abs_path) = Self::versioned_binary_paths(&platform)?;

        // 1) 先检查当前目录下是否已经有“同版本”的 server
        if fs::metadata(&binary_abs_path).is_ok_and(|stat| stat.is_file()) {
            self.cached_binary_path = Some(binary_abs_path.clone());
            return Ok(binary_abs_path);
        }

        // 2) 没有的话，只下载“同版本 tag”对应的 release
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release =
            match zed::github_release_by_tag_name(GITHUB_REPOSITORY, LANGUAGE_SERVER_RELEASE_TAG) {
                Ok(release) => release,
                Err(err) => {
                    let message = format!(
                        "failed to find language server release `{}`: {}",
                        LANGUAGE_SERVER_RELEASE_TAG, err
                    );
                    zed::set_language_server_installation_status(
                        language_server_id,
                        &zed::LanguageServerInstallationStatus::Failed(message.clone()),
                    );
                    return Err(message);
                }
            };

        let asset_name = format!(
            "{LANGUAGE_SERVER_BINARY_STEM}-{}-{}.{}",
            LANGUAGE_SERVER_RELEASE_TAG, platform.target, platform.archive_ext
        );

        let Some(asset) = release.assets.iter().find(|asset| asset.name == asset_name) else {
            let message = format!(
                "release `{}` exists, but asset `{}` is missing",
                LANGUAGE_SERVER_RELEASE_TAG, asset_name
            );
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Failed(message.clone()),
            );
            return Err(message);
        };

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        let version_dir = format!("{DOWNLOAD_ROOT_DIR}/{EXTENSION_VERSION}");
        let _ = fs::remove_dir_all(&version_dir);
        fs::create_dir_all(DOWNLOAD_ROOT_DIR)
            .map_err(|err| format!("failed to create download directory: {err}"))?;

        if let Err(err) =
            zed::download_file(&asset.download_url, &version_dir, platform.download_type)
        {
            let message = format!("failed to download `{asset_name}`: {err}");
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Failed(message.clone()),
            );
            return Err(message);
        }

        if platform.make_executable {
            if let Err(err) = zed::make_file_executable(&binary_rel_path) {
                let message = format!("failed to make `{binary_rel_path}` executable: {err}");
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Failed(message.clone()),
                );
                return Err(message);
            }
        }

        if !fs::metadata(&binary_abs_path).is_ok_and(|stat| stat.is_file()) {
            let message = format!(
                "downloaded `{asset_name}`, but `{}` was not found at the archive root",
                platform.binary_name
            );
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Failed(message.clone()),
            );
            return Err(message);
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );

        self.cached_binary_path = Some(binary_abs_path.clone());
        Ok(binary_abs_path)
    }
}

impl zed::Extension for UrdfExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.ensure_same_version_language_server(language_server_id)?,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(UrdfExtension);
