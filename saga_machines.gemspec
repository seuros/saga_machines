# frozen_string_literal: true
require_relative 'lib/saga_machines/version'

Gem::Specification.new do |s|
  s.name = 'saga_machines'
  s.version = SagaMachines::VERSION
  s.authors = ['Abdelkader Boudih']
  s.email = ['oss@seuros.com']
  s.summary = 'Initial native sequential saga and reverse compensation policies'
  s.description = 'Local unstable saga model, not a durable workflow or exactly-once runtime.'
  s.homepage = 'https://github.com/seuros/saga_machines'
  s.license = 'MIT'
  s.required_ruby_version = '>= 4.0.0'
  s.metadata['source_code_uri'] = s.homepage
  s.metadata['rubygems_mfa_required'] = 'true'
  s.metadata['cargo_crate_name'] = 'saga_machines_native'
  s.metadata['cargo_manifest_path'] = 'ext/saga_machines_native/ffi/Cargo.toml'
  s.files = Dir.glob(%w[
    lib/**/*.rb sig/**/*.rbs ext/**/*.{rs,rb,toml,md} Cargo.toml
    Cargo.lock README.md CHANGELOG.md LICENSE.txt
  ]).select { |p| File.file?(p) }
  s.require_paths = ['lib']
  s.extensions = ['ext/saga_machines_native/extconf.rb']
  s.add_dependency 'rb_sys', '~> 0.9', '>= 0.9.130'
end
